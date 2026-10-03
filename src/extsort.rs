use std::cmp::{Ordering, Reverse};
use std::collections::BinaryHeap;
use std::fs::File;
use std::io::{BufReader, BufWriter, Seek, SeekFrom};
use std::path::Path;

use csv::StringRecord;

use crate::diff::{Normalize, Rec, make_key};
use crate::progress::EVERY;
use crate::{Error, Header, Result};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

/// Rows of one file in key order.
pub enum Sorted {
    Memory(std::vec::IntoIter<Rec>),
    Merge(Merge),
}

impl Iterator for Sorted {
    type Item = Result<Rec>;

    fn next(&mut self) -> Option<Result<Rec>> {
        match self {
            Sorted::Memory(it) => it.next().map(Ok),
            Sorted::Merge(m) => m.next(),
        }
    }
}

/// Rough heap cost of a row: the text, the key copy, csv's per-field bounds
/// and the Vec/String headers. Close enough to keep the budget honest.
fn cost(r: &Rec) -> usize {
    r.row.as_slice().len() + r.key.len() + r.row.len() * 16 + 96
}

/// Sort a file's rows by key using at most about `budget` bytes of row data.
///
/// Rows are read into a buffer; each time the buffer passes the budget it is
/// sorted and written out as a run to an anonymous temp file. If nothing ever
/// spilled, the rows stay in memory. Otherwise the runs are merged lazily.
///
/// `sort_by` is stable and runs are numbered in file order, with the merge
/// breaking ties by run number, so rows sharing a key keep their file order.
pub fn sort_rows(
    header: &Header,
    mut reader: csv::Reader<File>,
    norm: &Normalize,
    budget: usize,
    tmp: &Path,
    progress: &AtomicU64,
) -> Result<(Sorted, u64)> {
    let mut buf: Vec<Rec> = Vec::new();
    let mut used = 0;
    let mut runs = Vec::new();
    let mut rows = 0u64;
    let mut row = csv::StringRecord::new();
    while reader
        .read_record(&mut row)
        .map_err(|e| csv_err(&header.path, e))?
    {
        rows += 1;
        if rows.is_multiple_of(EVERY) {
            progress.store(reader.position().byte(), Relaxed);
        }
        let row = std::mem::take(&mut row);
        let rec = Rec {
            key: make_key(&row, &header.key_idx, norm),
            row,
        };
        used += cost(&rec);
        buf.push(rec);
        if used >= budget {
            runs.push(spill(&mut buf, tmp)?);
            used = 0;
        }
    }
    buf.sort_by(|a, b| a.key.cmp(&b.key));
    if runs.is_empty() {
        return Ok((Sorted::Memory(buf.into_iter()), rows));
    }
    if !buf.is_empty() {
        runs.push(spill(&mut buf, tmp)?);
    }
    Ok((Sorted::Merge(Merge::new(runs, tmp)?), rows))
}

/// Write a sorted run as CSV: key first, then the row's fields.
fn spill(buf: &mut Vec<Rec>, tmp: &Path) -> Result<File> {
    buf.sort_by(|a, b| a.key.cmp(&b.key));
    let io = |err| Error::Io {
        path: tmp.to_path_buf(),
        err,
    };
    let mut file = tempfile::tempfile_in(tmp).map_err(io)?;
    {
        let mut w = csv::WriterBuilder::new()
            .flexible(true)
            .from_writer(BufWriter::new(&mut file));
        for r in buf.drain(..) {
            w.write_record(std::iter::once(r.key.as_str()).chain(r.row.iter()))
                .map_err(|e| csv_err(tmp, e))?;
        }
        w.flush().map_err(io)?;
    }
    file.seek(SeekFrom::Start(0)).map_err(io)?;
    Ok(file)
}

struct Head {
    key: String,
    run: usize,
    row: StringRecord,
}

impl PartialEq for Head {
    fn eq(&self, o: &Self) -> bool {
        self.cmp(o) == Ordering::Equal
    }
}
impl Eq for Head {}
impl PartialOrd for Head {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Head {
    fn cmp(&self, o: &Self) -> Ordering {
        self.key.cmp(&o.key).then(self.run.cmp(&o.run))
    }
}

/// k-way merge over sorted runs, holding one row per run in a min-heap.
pub struct Merge {
    runs: Vec<csv::StringRecordsIntoIter<BufReader<File>>>,
    heap: BinaryHeap<Reverse<Head>>,
    path: std::path::PathBuf,
}

impl Merge {
    fn new(files: Vec<File>, tmp: &Path) -> Result<Merge> {
        let mut m = Merge {
            runs: files
                .into_iter()
                .map(|f| {
                    csv::ReaderBuilder::new()
                        .has_headers(false)
                        .flexible(true)
                        .from_reader(BufReader::new(f))
                        .into_records()
                })
                .collect(),
            heap: BinaryHeap::new(),
            path: tmp.to_path_buf(),
        };
        for run in 0..m.runs.len() {
            m.refill(run)?;
        }
        Ok(m)
    }

    fn refill(&mut self, run: usize) -> Result<()> {
        if let Some(r) = self.runs[run].next() {
            let r = r.map_err(|e| csv_err(&self.path, e))?;
            let key = r.get(0).unwrap_or("").to_string();
            let row: StringRecord = r.iter().skip(1).collect();
            self.heap.push(Reverse(Head { key, run, row }));
        }
        Ok(())
    }

    fn next(&mut self) -> Option<Result<Rec>> {
        let Reverse(h) = self.heap.pop()?;
        if let Err(e) = self.refill(h.run) {
            return Some(Err(e));
        }
        Some(Ok(Rec {
            key: h.key,
            row: h.row,
        }))
    }
}

pub(crate) fn csv_err(path: &Path, err: csv::Error) -> Error {
    if let csv::ErrorKind::Utf8 { pos: Some(pos), .. } = err.kind() {
        return Error::NotUtf8 {
            path: path.to_path_buf(),
            line: pos.line(),
        };
    }
    Error::Csv {
        path: path.to_path_buf(),
        err,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Options, open};
    use std::io::Write;

    fn sorted(budget: usize, data: &str) -> Vec<(String, Vec<String>)> {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("in.csv");
        File::create(&path)
            .unwrap()
            .write_all(data.as_bytes())
            .unwrap();
        let opts = Options {
            key: vec!["id".into()],
            ..Options::default()
        };
        let (h, r) = open(&path, &opts).unwrap();
        sort_rows(
            &h,
            r,
            &Normalize::default(),
            budget,
            dir.path(),
            &AtomicU64::new(0),
        )
        .unwrap()
        .0
        .map(|r| {
            let r = r.unwrap();
            (r.key, r.row.iter().map(String::from).collect())
        })
        .collect()
    }

    #[test]
    fn spilled_merge_matches_in_memory_sort() {
        let data =
            "id,v\n5,e\n3,c\n9,\"has, comma\"\n1,a\n3,second\n7,\"line\nbreak\"\n,empty key\n";
        let mem = sorted(usize::MAX, data);
        // A 1-byte budget spills every row into its own run.
        let disk = sorted(1, data);
        assert_eq!(mem, disk);
        let keys: Vec<_> = disk.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["", "1", "3", "3", "5", "7", "9"]);
        // Equal keys keep file order across runs.
        assert_eq!(disk[2].1[1], "c");
        assert_eq!(disk[3].1[1], "second");
    }
}

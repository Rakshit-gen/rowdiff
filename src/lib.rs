//! Diff two CSV files by key.
//!
//! Both inputs are sorted by key (in memory when they fit, otherwise in
//! sorted runs on disk) and then walked side by side, so memory use is bounded
//! by the sort budget rather than by file size.

pub mod diff;
pub mod extsort;
pub mod output;

use std::fs::File;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Csv { path: PathBuf, source: csv::Error },
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("{path}: no column named {column:?} (columns are: {available})")]
    MissingKey { path: PathBuf, column: String, available: String },
    #[error("{path}: the file is empty, there is no header row")]
    NoHeader { path: PathBuf },
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone)]
pub struct Options {
    /// Column names that together identify a row.
    pub key: Vec<String>,
    /// Columns to leave out of the comparison.
    pub ignore: Vec<String>,
    pub delimiter: u8,
    pub normalize: diff::Normalize,
    /// Approximate bytes of row data to hold in memory across both files.
    /// Past this, rows are sorted on disk.
    pub memory: usize,
    /// Where sorted runs go. Defaults to the system temp directory.
    pub tmp_dir: Option<PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            key: Vec::new(),
            ignore: Vec::new(),
            delimiter: b',',
            normalize: diff::Normalize::default(),
            memory: 512 << 20,
            tmp_dir: None,
        }
    }
}

/// A file's header plus where its key columns sit.
#[derive(Debug, Clone)]
pub struct Header {
    pub path: PathBuf,
    pub columns: Vec<String>,
    pub key_idx: Vec<usize>,
}

impl Header {
    pub fn new(path: PathBuf, columns: Vec<String>, key: &[String]) -> Result<Header> {
        if columns.is_empty() {
            return Err(Error::NoHeader { path });
        }
        let mut key_idx = Vec::with_capacity(key.len());
        for k in key {
            match columns.iter().position(|c| c == k) {
                Some(i) => key_idx.push(i),
                None => {
                    return Err(Error::MissingKey {
                        available: columns.join(", "),
                        column: k.clone(),
                        path,
                    });
                }
            }
        }
        Ok(Header { path, columns, key_idx })
    }
}

/// Open a CSV file and read its header. Ragged rows are allowed; a missing
/// trailing cell reads as empty.
pub fn open(path: &Path, opts: &Options) -> Result<(Header, csv::Reader<File>)> {
    let file = File::open(path).map_err(|source| Error::Io { path: path.to_path_buf(), source })?;
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(opts.delimiter)
        .flexible(true)
        .from_reader(file);
    let columns = reader
        .headers()
        .map_err(|e| extsort::csv_err(path, e))?
        .iter()
        .map(String::from)
        .collect();
    let header = Header::new(path.to_path_buf(), columns, &opts.key)?;
    Ok((header, reader))
}

/// What a diff found, apart from the per-row changes handed to `emit`.
#[derive(Debug, Clone)]
pub struct Report {
    pub a: Header,
    pub b: Header,
    pub columns: diff::ColumnMap,
    pub summary: diff::Summary,
}

/// Two opened files with their headers read and columns matched up, ready to
/// diff. Splitting this from `run` lets callers see the columns before any
/// rows stream out.
pub struct Diff {
    pub a: Header,
    pub b: Header,
    pub columns: diff::ColumnMap,
    ra: csv::Reader<File>,
    rb: csv::Reader<File>,
    opts: Options,
}

impl Diff {
    pub fn prepare(a: &Path, b: &Path, opts: &Options) -> Result<Diff> {
        let (ha, ra) = open(a, opts)?;
        let (hb, rb) = open(b, opts)?;
        let columns = diff::ColumnMap::new(&ha, &hb, &opts.ignore);
        Ok(Diff { a: ha, b: hb, columns, ra, rb, opts: opts.clone() })
    }

    pub fn run(self, emit: impl FnMut(diff::Change)) -> Result<Report> {
        let tmp = self.opts.tmp_dir.clone().unwrap_or_else(std::env::temp_dir);
        let half = (self.opts.memory / 2).max(1);
        let n = &self.opts.normalize;
        let sa = extsort::sort_rows(&self.a, self.ra, n, half, &tmp)?;
        let sb = extsort::sort_rows(&self.b, self.rb, n, half, &tmp)?;
        let summary = diff::merge_join(sa, sb, &self.columns, n, emit)?;
        Ok(Report { a: self.a, b: self.b, columns: self.columns, summary })
    }
}

/// Diff two CSV files by `opts.key`, calling `emit` for every row that differs.
pub fn diff_files(a: &Path, b: &Path, opts: &Options, emit: impl FnMut(diff::Change)) -> Result<Report> {
    Diff::prepare(a, b, opts)?.run(emit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cols(s: &str) -> Vec<String> {
        s.split(',').map(String::from).collect()
    }

    #[test]
    fn finds_key_columns_by_name() {
        let h = Header::new("a.csv".into(), cols("id,name,region"), &cols("region,id")).unwrap();
        assert_eq!(h.key_idx, vec![2, 0]);
    }

    fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn diffs_two_files_the_same_way_in_memory_and_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let a = write(dir.path(), "a.csv", "sku,qty,price\nb2,1,9.50\na1,4,2.00\nc3,7,1.25\n");
        let b = write(dir.path(), "b.csv", "sku,price,qty\na1,2.00,5\nc3,1.25,7\nd4,3.00,1\n");
        for memory in [usize::MAX, 1] {
            let opts = Options { key: vec!["sku".into()], memory, ..Options::default() };
            let mut changes = Vec::new();
            let r = diff_files(&a, &b, &opts, |c| changes.push(c)).unwrap();
            let s = &r.summary;
            assert_eq!((s.added, s.removed, s.changed, s.unchanged), (1, 1, 1, 1), "memory={memory}");
            assert_eq!(changes.len(), 3);
        }
    }

    #[test]
    fn missing_key_lists_the_real_columns() {
        let err = Header::new("a.csv".into(), cols("id,name"), &cols("sku")).unwrap_err();
        assert_eq!(err.to_string(), r#"a.csv: no column named "sku" (columns are: id, name)"#);
    }
}

use std::fs::File;
use std::path::Path;

use crate::diff::{Rec, make_key};
use crate::{Error, Header, Result};

/// Rows of one file in key order.
pub enum Sorted {
    Memory(std::vec::IntoIter<Rec>),
}

impl Iterator for Sorted {
    type Item = Result<Rec>;

    fn next(&mut self) -> Option<Result<Rec>> {
        match self {
            Sorted::Memory(it) => it.next().map(Ok),
        }
    }
}

/// Read every row and sort by key. `sort_by` is stable, so rows sharing a key
/// keep their file order and the first one is the one that gets diffed.
pub fn sort_rows(header: &Header, mut reader: csv::Reader<File>) -> Result<Sorted> {
    let mut rows = Vec::new();
    for row in reader.records() {
        let row = row.map_err(|e| csv_err(&header.path, e))?;
        rows.push(Rec { key: make_key(&row, &header.key_idx), row });
    }
    rows.sort_by(|a, b| a.key.cmp(&b.key));
    Ok(Sorted::Memory(rows.into_iter()))
}

pub(crate) fn csv_err(path: &Path, source: csv::Error) -> Error {
    Error::Csv { path: path.to_path_buf(), source }
}

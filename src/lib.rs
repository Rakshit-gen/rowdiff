//! Diff two CSV files by key.
//!
//! Both inputs are sorted by key (in memory when they fit, otherwise in
//! sorted runs on disk) and then walked side by side, so memory use is bounded
//! by the sort budget rather than by file size.

pub mod diff;
pub mod extsort;

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
}

impl Default for Options {
    fn default() -> Self {
        Options { key: Vec::new(), ignore: Vec::new(), delimiter: b',' }
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

    #[test]
    fn missing_key_lists_the_real_columns() {
        let err = Header::new("a.csv".into(), cols("id,name"), &cols("sku")).unwrap_err();
        assert_eq!(err.to_string(), r#"a.csv: no column named "sku" (columns are: id, name)"#);
    }
}

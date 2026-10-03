//! Diff two CSV files by key.
//!
//! Both inputs are sorted by key (in memory when they fit, otherwise in
//! sorted runs on disk) and then walked side by side, so memory use is bounded
//! by the sort budget rather than by file size.

pub mod diff;

use std::path::PathBuf;

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

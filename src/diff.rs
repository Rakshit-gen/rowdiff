use std::cmp::Ordering;

use csv::StringRecord;

use crate::{Header, Result};

/// One row, with its key flattened to a single sortable string.
#[derive(Debug, Clone)]
pub struct Rec {
    pub key: String,
    pub row: StringRecord,
}

/// Separator for composite keys. A unit separator never shows up in real
/// CSV data, so "a" + "bc" and "ab" + "c" can't collide.
pub const KEY_SEP: char = '\u{1f}';

pub fn make_key(row: &StringRecord, key_idx: &[usize]) -> String {
    let mut key = String::new();
    for (n, &i) in key_idx.iter().enumerate() {
        if n > 0 {
            key.push(KEY_SEP);
        }
        key.push_str(row.get(i).unwrap_or(""));
    }
    key
}

/// Columns present in both files, by name, minus key and ignored columns.
#[derive(Debug, Clone)]
pub struct ColumnMap {
    /// (index in A, index in B, name)
    pub common: Vec<(usize, usize, String)>,
    pub only_a: Vec<String>,
    pub only_b: Vec<String>,
}

impl ColumnMap {
    pub fn new(a: &Header, b: &Header, ignore: &[String]) -> ColumnMap {
        let skip_a = |i: usize, name: &String| a.key_idx.contains(&i) || ignore.contains(name);
        let mut common = Vec::new();
        let mut only_a = Vec::new();
        for (ia, name) in a.columns.iter().enumerate() {
            if skip_a(ia, name) {
                continue;
            }
            match b.columns.iter().position(|c| c == name) {
                Some(ib) => common.push((ia, ib, name.clone())),
                None => only_a.push(name.clone()),
            }
        }
        let only_b = b
            .columns
            .iter()
            .enumerate()
            .filter(|(ib, name)| {
                !b.key_idx.contains(ib) && !ignore.contains(name) && !a.columns.contains(name)
            })
            .map(|(_, n)| n.clone())
            .collect();
        ColumnMap { common, only_a, only_b }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CellChange {
    /// Index into `ColumnMap::common`.
    pub col: usize,
    pub old: String,
    pub new: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    Added { key: String, row: StringRecord },
    Removed { key: String, row: StringRecord },
    Changed { key: String, cells: Vec<CellChange> },
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Summary {
    pub rows_a: u64,
    pub rows_b: u64,
    pub added: u64,
    pub removed: u64,
    pub changed: u64,
    pub unchanged: u64,
    /// Changed-row count per entry of `ColumnMap::common`.
    pub per_column: Vec<u64>,
}

/// Walk two key-sorted streams side by side.
pub fn merge_join<A, B>(a: A, b: B, cols: &ColumnMap, mut emit: impl FnMut(Change)) -> Result<Summary>
where
    A: IntoIterator<Item = Result<Rec>>,
    B: IntoIterator<Item = Result<Rec>>,
{
    let mut a = a.into_iter();
    let mut b = b.into_iter();
    let mut s = Summary { per_column: vec![0; cols.common.len()], ..Summary::default() };
    let mut x = a.next().transpose()?;
    let mut y = b.next().transpose()?;

    loop {
        let ord = match (&x, &y) {
            (None, None) => break,
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (Some(ra), Some(rb)) => ra.key.cmp(&rb.key),
        };
        match ord {
            Ordering::Less => {
                let r = x.take().unwrap();
                s.rows_a += 1;
                s.removed += 1;
                emit(Change::Removed { key: r.key, row: r.row });
                x = a.next().transpose()?;
            }
            Ordering::Greater => {
                let r = y.take().unwrap();
                s.rows_b += 1;
                s.added += 1;
                emit(Change::Added { key: r.key, row: r.row });
                y = b.next().transpose()?;
            }
            Ordering::Equal => {
                let (ra, rb) = (x.take().unwrap(), y.take().unwrap());
                s.rows_a += 1;
                s.rows_b += 1;
                let cells = compare(&ra.row, &rb.row, cols);
                if cells.is_empty() {
                    s.unchanged += 1;
                } else {
                    s.changed += 1;
                    for c in &cells {
                        s.per_column[c.col] += 1;
                    }
                    emit(Change::Changed { key: ra.key, cells });
                }
                x = a.next().transpose()?;
                y = b.next().transpose()?;
            }
        }
    }
    Ok(s)
}

fn compare(a: &StringRecord, b: &StringRecord, cols: &ColumnMap) -> Vec<CellChange> {
    let mut out = Vec::new();
    for (n, (ia, ib, _)) in cols.common.iter().enumerate() {
        let (va, vb) = (a.get(*ia).unwrap_or(""), b.get(*ib).unwrap_or(""));
        if va != vb {
            out.push(CellChange { col: n, old: va.to_string(), new: vb.to_string() });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(cols: &str, key: &str) -> Header {
        Header::new(
            "t.csv".into(),
            cols.split(',').map(String::from).collect(),
            &key.split(',').map(String::from).collect::<Vec<_>>(),
        )
        .unwrap()
    }

    fn recs(h: &Header, rows: &[&str]) -> Vec<Result<Rec>> {
        rows.iter()
            .map(|r| {
                let row = StringRecord::from(r.split(',').collect::<Vec<_>>());
                Ok(Rec { key: make_key(&row, &h.key_idx), row })
            })
            .collect()
    }

    #[test]
    fn added_removed_changed_and_unchanged() {
        let ha = header("id,name,price", "id");
        let hb = header("id,price,name", "id");
        let cols = ColumnMap::new(&ha, &hb, &[]);
        let a = recs(&ha, &["1,pen,10", "2,cup,5", "3,mug,7"]);
        let b = recs(&hb, &["2,6,cup", "3,7,mug", "4,1,pad"]);
        let mut out = Vec::new();
        let s = merge_join(a, b, &cols, |c| out.push(c)).unwrap();

        assert_eq!((s.added, s.removed, s.changed, s.unchanged), (1, 1, 1, 1));
        assert!(matches!(&out[0], Change::Removed { key, .. } if key == "1"));
        assert_eq!(
            out[1],
            Change::Changed { key: "2".into(), cells: vec![CellChange { col: 1, old: "5".into(), new: "6".into() }] }
        );
        assert!(matches!(&out[2], Change::Added { key, .. } if key == "4"));
        assert_eq!(s.per_column, vec![0, 1]);
    }

    #[test]
    fn columns_only_in_one_file_are_reported_not_diffed() {
        let ha = header("id,name,legacy", "id");
        let hb = header("id,name,email,notes", "id");
        let cols = ColumnMap::new(&ha, &hb, &["notes".into()]);
        assert_eq!(cols.only_a, vec!["legacy"]);
        assert_eq!(cols.only_b, vec!["email"]);
        assert_eq!(cols.common.len(), 1);
    }

    #[test]
    fn composite_keys_do_not_collide() {
        let h = header("a,b", "a,b");
        let r1 = StringRecord::from(vec!["a", "bc"]);
        let r2 = StringRecord::from(vec!["ab", "c"]);
        assert_ne!(make_key(&r1, &h.key_idx), make_key(&r2, &h.key_idx));
    }
}

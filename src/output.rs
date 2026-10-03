//! Machine-readable forms of a change, shared by the CLI and the server.

use serde_json::{Map, Value, json};

use crate::Header;
use crate::diff::{Change, ColumnMap, KEY_SEP, Side, Summary};

pub fn key_parts(key: &str) -> Vec<&str> {
    key.split(KEY_SEP).collect()
}

fn row_object(h: &Header, row: &csv::StringRecord) -> Value {
    let mut m = Map::new();
    for (name, v) in h.columns.iter().zip(row.iter()) {
        m.insert(name.clone(), Value::String(v.to_string()));
    }
    Value::Object(m)
}

/// One change as a JSON object, with column names spelled out.
pub fn change_json(a: &Header, b: &Header, cols: &ColumnMap, c: &Change) -> Value {
    match c {
        Change::Added { key, row } => {
            json!({ "kind": "added", "key": key_parts(key), "row": row_object(b, row) })
        }
        Change::Removed { key, row } => {
            json!({ "kind": "removed", "key": key_parts(key), "row": row_object(a, row) })
        }
        Change::Changed { key, cells, row } => json!({
            "kind": "changed",
            "key": key_parts(key),
            "row": row_object(b, row),
            "cells": cells.iter().map(|c| json!({
                "column": cols.common[c.col].2,
                "old": c.old,
                "new": c.new,
            })).collect::<Vec<_>>(),
        }),
        Change::Duplicate { side, key, row } => {
            let (file, h) = if *side == Side::A { ("a", a) } else { ("b", b) };
            json!({ "kind": "duplicate", "file": file, "key": key_parts(key), "row": row_object(h, row) })
        }
    }
}

/// One change as CSV lines of `kind,key,column,old,new`: a line per changed
/// cell, one line for an added, removed or repeated row.
pub fn change_csv_rows(cols: &ColumnMap, c: &Change) -> Vec<[String; 5]> {
    let k = |key: &str| key_parts(key).join(" | ");
    match c {
        Change::Added { key, .. } => vec![[
            "added".into(),
            k(key),
            String::new(),
            String::new(),
            String::new(),
        ]],
        Change::Removed { key, .. } => vec![[
            "removed".into(),
            k(key),
            String::new(),
            String::new(),
            String::new(),
        ]],
        Change::Duplicate { side, key, .. } => {
            let what = if *side == Side::A {
                "repeated in a"
            } else {
                "repeated in b"
            };
            vec![[
                what.into(),
                k(key),
                String::new(),
                String::new(),
                String::new(),
            ]]
        }
        Change::Changed { key, cells, .. } => cells
            .iter()
            .map(|c| {
                [
                    "changed".into(),
                    k(key),
                    cols.common[c.col].2.clone(),
                    c.old.clone(),
                    c.new.clone(),
                ]
            })
            .collect(),
    }
}

/// Totals for a finished diff, including changed-row counts per column in
/// the order the columns appear in the first file.
pub fn summary_json(cols: &ColumnMap, s: &Summary) -> Value {
    let per_column: Map<String, Value> = cols
        .common
        .iter()
        .zip(&s.per_column)
        .map(|((_, _, name), n)| (name.clone(), (*n).into()))
        .collect();
    json!({
        "rows_a": s.rows_a, "rows_b": s.rows_b,
        "added": s.added, "removed": s.removed, "changed": s.changed, "unchanged": s.unchanged,
        "duplicates_a": s.duplicates_a, "duplicates_b": s.duplicates_b,
        "only_in_a": cols.only_a, "only_in_b": cols.only_b,
        "changed_by_column": per_column,
    })
}

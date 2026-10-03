//! Runs the built binary the way a shell script would: files in, exit code
//! and stdout out.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const A: &str = "sku,qty,price\nb2,1,9.50\na1,4,2.00\nc3,7,1.25\n";
const B: &str = "sku,price,qty,note\na1,2.00,5,\nc3,1.25,7,x\nd4,3.00,1,\n";

fn files(dir: &Path) -> (PathBuf, PathBuf) {
    let (a, b) = (dir.join("a.csv"), dir.join("b.csv"));
    std::fs::write(&a, A).unwrap();
    std::fs::write(&b, B).unwrap();
    (a, b)
}

fn rowdiff(args: &[&Path], extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rowdiff"))
        .args(args)
        .args(extra)
        .output()
        .unwrap()
}

#[test]
fn differing_files_exit_1_with_csv_changes() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = files(dir.path());
    let out = rowdiff(&[&a, &b], &["-k", "sku", "-f", "csv"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(out.stdout).unwrap(),
        "kind,key,column,old,new\nchanged,a1,qty,4,5\nremoved,b2,,,\nadded,d4,,,\n"
    );
}

#[test]
fn identical_files_exit_0() {
    let dir = tempfile::tempdir().unwrap();
    let (a, _) = files(dir.path());
    let out = rowdiff(&[&a, &a], &["-k", "sku"]);
    assert_eq!(out.status.code(), Some(0));
}

#[test]
fn ignored_column_hides_the_change() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = files(dir.path());
    let out = rowdiff(&[&a, &b], &["-k", "sku", "-i", "qty", "-f", "csv"]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(!text.contains("changed,"), "{text}");
}

#[test]
fn jsonl_ends_with_summary() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = files(dir.path());
    let out = rowdiff(&[&a, &b], &["-k", "sku", "-f", "jsonl"]);
    let text = String::from_utf8(out.stdout).unwrap();
    let last: serde_json::Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
    assert_eq!(last["kind"], "summary");
    assert_eq!(last["changed"], 1);
    assert_eq!(last["only_in_b"][0], "note");
}

#[test]
fn unknown_key_exits_2_and_lists_columns() {
    let dir = tempfile::tempdir().unwrap();
    let (a, b) = files(dir.path());
    let out = rowdiff(&[&a, &b], &["-k", "id"]);
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(err.contains("columns are: sku, qty, price"), "{err}");
}

#[test]
fn missing_file_exits_2() {
    let dir = tempfile::tempdir().unwrap();
    let (a, _) = files(dir.path());
    let out = rowdiff(&[&a, &dir.path().join("nope.csv")], &["-k", "sku"]);
    assert_eq!(out.status.code(), Some(2));
}

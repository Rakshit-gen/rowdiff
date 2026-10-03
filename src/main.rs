use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Result, bail};
use clap::{Parser, ValueEnum};
use rowdiff::diff::{Change, KEY_SEP, Normalize, Side};
use rowdiff::output::{change_csv_rows, change_json};
use rowdiff::{Diff, Options, Report};

/// Compare two CSV exports by key.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// The older file.
    a: PathBuf,
    /// The newer file.
    b: PathBuf,
    /// Column that identifies a row. Repeat it for a composite key.
    #[arg(short, long, required = true)]
    key: Vec<String>,
    /// Column to leave out of the comparison. Can be repeated.
    #[arg(short, long)]
    ignore: Vec<String>,
    /// Field delimiter, one byte. Use '\t' for tabs.
    #[arg(short, long, default_value = ",")]
    delimiter: String,
    /// Ignore leading and trailing spaces in keys and values.
    #[arg(long)]
    trim: bool,
    /// Treat "Pen" and "pen" as the same value.
    #[arg(long)]
    ignore_case: bool,
    /// Numbers within this distance count as equal. 0 still treats 1.0 and 1 as the same.
    #[arg(long)]
    tolerance: Option<f64>,
    /// Memory for sorting before it spills to disk, like 512M or 2G.
    #[arg(long, default_value = "512M", value_parser = parse_size)]
    memory: usize,
    /// text for people; jsonl or csv print every change for other tools.
    #[arg(short, long, value_enum, default_value_t = Format::Text)]
    format: Format,
    /// How many changed rows to print in text format.
    #[arg(long, default_value_t = 20)]
    limit: usize,
}

#[derive(Clone, Copy, PartialEq, ValueEnum)]
enum Format {
    Text,
    Jsonl,
    Csv,
}

fn parse_size(s: &str) -> Result<usize, String> {
    let s = s.trim();
    let (num, mul) = match s.char_indices().last() {
        Some((i, 'K' | 'k')) => (&s[..i], 1 << 10),
        Some((i, 'M' | 'm')) => (&s[..i], 1 << 20),
        Some((i, 'G' | 'g')) => (&s[..i], 1 << 30),
        _ => (s, 1),
    };
    num.parse::<usize>()
        .map(|n| n * mul)
        .map_err(|_| format!("{s:?} is not a size like 512M or 2G"))
}

fn delimiter(s: &str) -> Result<u8> {
    match s {
        "\\t" | "\t" | "tab" => Ok(b'\t'),
        _ if s.len() == 1 => Ok(s.as_bytes()[0]),
        _ => bail!("the delimiter has to be a single byte, got {s:?}"),
    }
}

fn show_key(k: &str) -> String {
    k.replace(KEY_SEP, ", ")
}

/// Same convention as diff(1): 0 when the files match, 1 when they differ,
/// 2 when something went wrong.
fn main() -> ExitCode {
    match run() {
        Ok(false) => ExitCode::SUCCESS,
        Ok(true) => ExitCode::from(1),
        Err(e) => {
            eprintln!("rowdiff: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<bool> {
    let cli = Cli::parse();
    let opts = Options {
        key: cli.key,
        ignore: cli.ignore,
        delimiter: delimiter(&cli.delimiter)?,
        normalize: Normalize { trim: cli.trim, ignore_case: cli.ignore_case, tolerance: cli.tolerance },
        memory: cli.memory,
        tmp_dir: None,
    };

    let d = Diff::prepare(&cli.a, &cli.b, &opts)?;
    let report = match cli.format {
        Format::Text => text(d, cli.limit)?,
        Format::Jsonl => jsonl(d)?,
        Format::Csv => csv_out(d)?,
    };
    let s = &report.summary;
    Ok(s.added + s.removed + s.changed + s.duplicates_a + s.duplicates_b > 0)
}

fn text(d: Diff, limit: usize) -> Result<Report> {
    let mut shown = Vec::new();
    let mut hidden = 0u64;
    let report = d.run(|c| {
        if shown.len() < limit {
            shown.push(c);
        } else {
            hidden += 1;
        }
    })?;
    let mut out = std::io::stdout().lock();
    print_summary(&mut out, &report)?;
    if !shown.is_empty() {
        writeln!(out)?;
    }
    for c in &shown {
        print_change(&mut out, &report, c)?;
    }
    if hidden > 0 {
        writeln!(out, "... and {hidden} more. Raise --limit to see them.")?;
    }
    Ok(report)
}

/// Every change as one JSON object per line, then a summary line.
fn jsonl(d: Diff) -> Result<Report> {
    let (a, b, cols) = (d.a.clone(), d.b.clone(), d.columns.clone());
    let mut out = std::io::BufWriter::new(std::io::stdout().lock());
    let mut err = None;
    let report = d.run(|c| {
        if err.is_none()
            && let Err(e) = writeln!(out, "{}", change_json(&a, &b, &cols, &c))
        {
            err = Some(e);
        }
    })?;
    if let Some(e) = err {
        return Err(e.into());
    }
    let s = &report.summary;
    let per_column: serde_json::Map<_, _> = cols
        .common
        .iter()
        .zip(&s.per_column)
        .map(|((_, _, name), n)| (name.clone(), (*n).into()))
        .collect();
    writeln!(
        out,
        "{}",
        serde_json::json!({
            "kind": "summary",
            "rows_a": s.rows_a, "rows_b": s.rows_b,
            "added": s.added, "removed": s.removed, "changed": s.changed, "unchanged": s.unchanged,
            "duplicates_a": s.duplicates_a, "duplicates_b": s.duplicates_b,
            "only_in_a": cols.only_a, "only_in_b": cols.only_b,
            "changed_by_column": per_column,
        })
    )?;
    out.flush()?;
    Ok(report)
}

/// Every change as `kind,key,column,old,new`, one line per changed cell.
fn csv_out(d: Diff) -> Result<Report> {
    let cols = d.columns.clone();
    let mut w = csv::Writer::from_writer(std::io::stdout().lock());
    w.write_record(["kind", "key", "column", "old", "new"])?;
    let mut err = None;
    let report = d.run(|c| {
        for row in change_csv_rows(&cols, &c) {
            if err.is_none()
                && let Err(e) = w.write_record(&row)
            {
                err = Some(e);
            }
        }
    })?;
    if let Some(e) = err {
        return Err(e.into());
    }
    w.flush()?;
    Ok(report)
}

fn print_summary(out: &mut impl Write, r: &Report) -> std::io::Result<()> {
    let s = &r.summary;
    writeln!(out, "{}  {} rows", r.a.path.display(), s.rows_a)?;
    writeln!(out, "{}  {} rows", r.b.path.display(), s.rows_b)?;
    writeln!(out)?;
    writeln!(out, "added      {}", s.added)?;
    writeln!(out, "removed    {}", s.removed)?;
    writeln!(out, "changed    {}", s.changed)?;
    writeln!(out, "unchanged  {}", s.unchanged)?;
    if s.duplicates_a + s.duplicates_b > 0 {
        writeln!(
            out,
            "repeated keys: {} in {}, {} in {}. Only the first row with each key was compared.",
            s.duplicates_a,
            r.a.path.display(),
            s.duplicates_b,
            r.b.path.display()
        )?;
    }
    if !r.columns.only_a.is_empty() {
        writeln!(out, "columns only in {}: {}", r.a.path.display(), r.columns.only_a.join(", "))?;
    }
    if !r.columns.only_b.is_empty() {
        writeln!(out, "columns only in {}: {}", r.b.path.display(), r.columns.only_b.join(", "))?;
    }
    if s.changed > 0 {
        writeln!(out, "\nchanged rows by column")?;
        let width = r.columns.common.iter().map(|c| c.2.len()).max().unwrap_or(0);
        let mut by_col: Vec<_> = r.columns.common.iter().zip(&s.per_column).filter(|(_, n)| **n > 0).collect();
        by_col.sort_by(|x, y| y.1.cmp(x.1));
        for ((_, _, name), n) in by_col {
            writeln!(out, "  {name:width$}  {n}")?;
        }
    }
    Ok(())
}

fn print_change(out: &mut impl Write, r: &Report, c: &Change) -> std::io::Result<()> {
    match c {
        Change::Added { key, .. } => writeln!(out, "+ {}", show_key(key)),
        Change::Removed { key, .. } => writeln!(out, "- {}", show_key(key)),
        Change::Changed { key, cells } => {
            let parts: Vec<_> = cells
                .iter()
                .map(|cell| format!("{}: {:?} -> {:?}", r.columns.common[cell.col].2, cell.old, cell.new))
                .collect();
            writeln!(out, "~ {}  {}", show_key(key), parts.join(", "))
        }
        Change::Duplicate { side, key, .. } => {
            let file = if *side == Side::A { &r.a.path } else { &r.b.path };
            writeln!(out, "! {}  repeated in {}", show_key(key), file.display())
        }
    }
}

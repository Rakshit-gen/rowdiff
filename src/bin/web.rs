//! rowdiff in the browser: upload two CSV files, browse the differences.
//!
//!     cargo run --release --features web --bin rowdiff-web
//!
//! Binds to 127.0.0.1 by default. Uploaded files and results live in a temp
//! directory that is removed when the server stops.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use axum::extract::{DefaultBodyLimit, Multipart, Path as UrlPath, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Parser;
use rowdiff::diff::Change;
use rowdiff::output::{change_csv_rows, change_json, summary_json};
use rowdiff::progress::Progress;
use rowdiff::{Diff, Options, Report};
use serde_json::{Value, json};
use tokio::io::AsyncWriteExt;
use tower_http::services::{ServeDir, ServeFile};

#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Address to listen on.
    #[arg(long, default_value = "127.0.0.1:7878")]
    addr: SocketAddr,
    /// Serve the UI from this directory instead of the copy built into the
    /// binary. Handy while working on the UI.
    #[arg(long)]
    static_dir: Option<PathBuf>,
    /// Largest upload accepted, in megabytes, for both files together.
    #[arg(long, default_value_t = 4096)]
    max_upload_mb: usize,
}

struct App {
    dir: tempfile::TempDir,
    next: AtomicU64,
    jobs: Mutex<HashMap<u64, Arc<Job>>>,
}

struct Job {
    key: Vec<String>,
    name_a: String,
    name_b: String,
    columns_a: Vec<String>,
    columns_b: Vec<String>,
    progress: Progress,
    state: Mutex<JobState>,
}

enum JobState {
    Running,
    Failed(String),
    Done(Arc<Results>),
}

const KINDS: [&str; 4] = ["added", "removed", "changed", "duplicate"];

/// A finished diff. Changes live one JSON object per line in `file`; the
/// vectors here are just positions into it, so a page of any filtered view is
/// a few seeks no matter how large the diff is. Positions are u32, which caps
/// a single diff at about four billion changed rows.
struct Results {
    report: Report,
    file: PathBuf,
    /// The same changes as `kind,key,column,old,new`, for download.
    csv: PathBuf,
    /// Byte offset where each line starts, plus one past the last line.
    offsets: Vec<u64>,
    by_kind: [Vec<u32>; 4],
    /// For each compared column, the changed rows that touch it.
    by_column: Vec<Vec<u32>>,
}

impl Results {
    fn read_lines(&self, ids: &[u32]) -> std::io::Result<Vec<Value>> {
        use std::io::{Read, Seek, SeekFrom};
        let mut f = std::fs::File::open(&self.file)?;
        let mut out = Vec::with_capacity(ids.len());
        let mut buf = Vec::new();
        for &i in ids {
            let (start, end) = (self.offsets[i as usize], self.offsets[i as usize + 1]);
            f.seek(SeekFrom::Start(start))?;
            buf.resize((end - start - 1) as usize, 0);
            f.read_exact(&mut buf)?;
            out.push(serde_json::from_slice(&buf)?);
        }
        Ok(out)
    }
}

/// Run a prepared diff, writing every change to `dir/changes.jsonl` and
/// indexing it as it goes.
fn run_job(d: Diff, progress: &Progress, dir: &Path) -> anyhow::Result<Results> {
    use std::io::Write;
    let file = dir.join("changes.jsonl");
    let mut w = std::io::BufWriter::new(std::fs::File::create(&file)?);
    let csv_path = dir.join("changes.csv");
    let mut cw = csv::Writer::from_path(&csv_path)?;
    cw.write_record(["kind", "key", "column", "old", "new"])?;
    let (a, b, cols) = (d.a.clone(), d.b.clone(), d.columns.clone());
    let mut offsets = vec![0u64];
    let mut by_kind: [Vec<u32>; 4] = Default::default();
    let mut by_column = vec![Vec::new(); cols.common.len()];
    let mut failed: Option<anyhow::Error> = None;

    let report = d.run_with(progress, |c| {
        if failed.is_some() {
            return;
        }
        let n = (offsets.len() - 1) as u32;
        let kind = match &c {
            Change::Added { .. } => 0,
            Change::Removed { .. } => 1,
            Change::Changed { cells, .. } => {
                for cell in cells {
                    by_column[cell.col].push(n);
                }
                2
            }
            Change::Duplicate { .. } => 3,
        };
        by_kind[kind].push(n);
        for row in change_csv_rows(&cols, &c) {
            if let Err(e) = cw.write_record(&row) {
                failed = Some(e.into());
            }
        }
        let mut line = change_json(&a, &b, &cols, &c).to_string();
        line.push('\n');
        if let Err(e) = w.write_all(line.as_bytes()) {
            failed = Some(e.into());
        }
        offsets.push(offsets[n as usize] + line.len() as u64);
    })?;
    if let Some(e) = failed {
        return Err(e);
    }
    w.flush()?;
    cw.flush()?;
    Ok(Results {
        report,
        file,
        csv: csv_path,
        offsets,
        by_kind,
        by_column,
    })
}

/// The UI from web/dist, compiled in so the binary runs on its own. Build the
/// UI first (`pnpm --dir web build`); without it the binary still builds and
/// the API works, but the pages are missing.
#[derive(rust_embed::RustEmbed)]
#[folder = "web/dist"]
#[allow_missing = true]
struct Ui;

async fn embedded_ui(uri: axum::http::Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let (file, path) = match Ui::get(path) {
        Some(f) if !path.is_empty() => (f, path),
        // Unknown paths get the app shell; the app reads ?diff= itself.
        _ => match Ui::get("index.html") {
            Some(f) => (f, "index.html"),
            None => {
                return (
                    StatusCode::NOT_FOUND,
                    "This build has no UI. Run `pnpm --dir web build` and rebuild, or pass --static-dir.",
                )
                    .into_response();
            }
        },
    };
    let cache = if path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    (
        [
            (
                axum::http::header::CONTENT_TYPE,
                file.metadata.mimetype().to_string(),
            ),
            (axum::http::header::CACHE_CONTROL, cache.to_string()),
        ],
        file.data,
    )
        .into_response()
}

/// An error the browser can show as is.
struct ApiError(StatusCode, String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

fn bad(msg: impl Into<String>) -> ApiError {
    ApiError(StatusCode::BAD_REQUEST, msg.into())
}

fn internal(e: impl std::fmt::Display) -> ApiError {
    ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())
}

/// Keep only the last path component of an uploaded file's name, so a name
/// like "../../etc/x" can't point outside the job directory.
fn clean_name(raw: Option<&str>, fallback: &str) -> String {
    raw.and_then(|n| Path::new(n).file_name())
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty())
        .unwrap_or(fallback)
        .to_string()
}

/// Accept two files and options as multipart form fields, check that the
/// key columns exist, and start the diff.
async fn create(
    State(app): State<Arc<App>>,
    mut form: Multipart,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let id = app.next.fetch_add(1, Ordering::Relaxed);
    let dir = app.dir.path().join(id.to_string());
    let mut opts = Options::default();
    let mut files: [Option<(String, PathBuf)>; 2] = [None, None];

    while let Some(mut field) = form.next_field().await.map_err(|e| bad(e.body_text()))? {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "a" | "b" => {
                let slot = usize::from(name == "b");
                let shown =
                    clean_name(field.file_name(), if slot == 0 { "a.csv" } else { "b.csv" });
                let side_dir = dir.join(&name);
                tokio::fs::create_dir_all(&side_dir)
                    .await
                    .map_err(internal)?;
                let path = side_dir.join(&shown);
                let mut out = tokio::fs::File::create(&path).await.map_err(internal)?;
                while let Some(chunk) = field.chunk().await.map_err(|e| bad(e.body_text()))? {
                    out.write_all(&chunk).await.map_err(internal)?;
                }
                out.flush().await.map_err(internal)?;
                files[slot] = Some((shown, path));
            }
            other => {
                let v = field.text().await.map_err(|e| bad(e.body_text()))?;
                match other {
                    "key" if !v.is_empty() => opts.key.push(v),
                    "ignore" if !v.is_empty() => opts.ignore.push(v),
                    "trim" => opts.normalize.trim = v == "true",
                    "ignore_case" => opts.normalize.ignore_case = v == "true",
                    "tolerance" if !v.is_empty() => {
                        let t = v
                            .parse()
                            .map_err(|_| bad(format!("tolerance {v:?} is not a number")))?;
                        opts.normalize.tolerance = Some(t);
                    }
                    "delimiter" => match v.as_bytes() {
                        [b] => opts.delimiter = *b,
                        _ if v == "\\t" || v == "tab" => opts.delimiter = b'\t',
                        _ => return Err(bad("the delimiter has to be one character")),
                    },
                    _ => {}
                }
            }
        }
    }

    let [Some((name_a, path_a)), Some((name_b, path_b))] = files else {
        return Err(bad("send two files, as fields a and b"));
    };
    if opts.key.is_empty() {
        return Err(bad("pick at least one key column"));
    }

    let (pa, pb) = (path_a.clone(), path_b.clone());
    let o = opts.clone();
    let prepared = tokio::task::spawn_blocking(move || Diff::prepare(&pa, &pb, &o))
        .await
        .map_err(internal)?;
    let d = prepared.map_err(|e| {
        // Show the names the user picked, not where the server keeps them.
        let msg = e
            .to_string()
            .replace(&path_a.display().to_string(), &name_a)
            .replace(&path_b.display().to_string(), &name_b);
        bad(msg)
    })?;

    let job = Arc::new(Job {
        key: opts.key.clone(),
        name_a,
        name_b,
        columns_a: d.a.columns.clone(),
        columns_b: d.b.columns.clone(),
        progress: Progress::default(),
        state: Mutex::new(JobState::Running),
    });
    app.jobs.lock().unwrap().insert(id, job.clone());
    let body = status_json(id, &job);

    tokio::task::spawn_blocking(move || {
        let result = run_job(d, &job.progress, &dir);
        *job.state.lock().unwrap() = match result {
            Ok(r) => JobState::Done(Arc::new(r)),
            Err(e) => JobState::Failed(format!("{e:#}")),
        };
    });
    Ok((StatusCode::CREATED, Json(body)))
}

fn status_json(id: u64, job: &Job) -> Value {
    let (phase, done, total) = job.progress.snapshot();
    let mut v = json!({
        "id": id,
        "key": job.key,
        "a": { "name": job.name_a, "columns": job.columns_a },
        "b": { "name": job.name_b, "columns": job.columns_b },
        "phase": phase,
        "done": done,
        "total": total,
    });
    match &*job.state.lock().unwrap() {
        JobState::Running => v["status"] = "running".into(),
        JobState::Failed(e) => {
            v["status"] = "failed".into();
            v["error"] = e.as_str().into();
        }
        JobState::Done(r) => {
            let r = &r.report;
            v["status"] = "done".into();
            v["summary"] = summary_json(&r.columns, &r.summary);
            v["compared_columns"] = r
                .columns
                .common
                .iter()
                .map(|c| c.2.clone())
                .collect::<Vec<_>>()
                .into();
        }
    }
    v
}

fn find(app: &App, id: u64) -> Result<Arc<Job>, ApiError> {
    app.jobs.lock().unwrap().get(&id).cloned().ok_or_else(|| {
        ApiError(
            StatusCode::NOT_FOUND,
            format!("no diff with id {id}, it may have been deleted"),
        )
    })
}

async fn status(
    State(app): State<Arc<App>>,
    UrlPath(id): UrlPath<u64>,
) -> Result<Json<Value>, ApiError> {
    let job = find(&app, id)?;
    Ok(Json(status_json(id, &job)))
}

#[derive(serde::Deserialize)]
struct RowsQuery {
    /// all, added, removed, changed or duplicate.
    kind: Option<String>,
    /// Only changed rows where this column changed.
    column: Option<String>,
    #[serde(default)]
    offset: usize,
    limit: Option<usize>,
}

/// One page of changes, optionally narrowed to a kind or a column.
async fn rows(
    State(app): State<Arc<App>>,
    UrlPath(id): UrlPath<u64>,
    Query(q): Query<RowsQuery>,
) -> Result<Json<Value>, ApiError> {
    let job = find(&app, id)?;
    let results = match &*job.state.lock().unwrap() {
        JobState::Done(r) => r.clone(),
        JobState::Running => {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "this diff is still running".into(),
            ));
        }
        JobState::Failed(e) => return Err(ApiError(StatusCode::CONFLICT, e.clone())),
    };
    let limit = q.limit.unwrap_or(200).min(1000);

    let all: Vec<u32>;
    let ids: &[u32] = if let Some(col) = &q.column {
        let i = results
            .report
            .columns
            .common
            .iter()
            .position(|c| &c.2 == col)
            .ok_or_else(|| bad(format!("{col:?} is not one of the compared columns")))?;
        &results.by_column[i]
    } else {
        match q.kind.as_deref().unwrap_or("all") {
            "all" => {
                all = (0..(results.offsets.len() - 1) as u32).collect();
                &all
            }
            k => {
                let i = KINDS.iter().position(|x| *x == k).ok_or_else(|| {
                    bad(format!(
                        "kind has to be all, {}, not {k:?}",
                        KINDS.join(", ")
                    ))
                })?;
                &results.by_kind[i]
            }
        }
    };
    let total = ids.len();
    let page: Vec<u32> = ids.iter().skip(q.offset).take(limit).copied().collect();
    let r = results.clone();
    let lines = tokio::task::spawn_blocking(move || r.read_lines(&page))
        .await
        .map_err(internal)?
        .map_err(internal)?;
    Ok(Json(
        json!({ "total": total, "offset": q.offset, "rows": lines }),
    ))
}

fn finished(job: &Job) -> Result<Arc<Results>, ApiError> {
    match &*job.state.lock().unwrap() {
        JobState::Done(r) => Ok(r.clone()),
        JobState::Running => Err(ApiError(
            StatusCode::CONFLICT,
            "this diff is still running".into(),
        )),
        JobState::Failed(e) => Err(ApiError(StatusCode::CONFLICT, e.clone())),
    }
}

/// Every change as CSV, named after the two files.
async fn export(
    State(app): State<Arc<App>>,
    UrlPath(id): UrlPath<u64>,
) -> Result<Response, ApiError> {
    let job = find(&app, id)?;
    let r = finished(&job)?;
    let file = tokio::fs::File::open(&r.csv).await.map_err(internal)?;
    let stem = |n: &str| {
        Path::new(n)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("file")
            .replace('"', "")
    };
    let name = format!("{}-vs-{}.csv", stem(&job.name_a), stem(&job.name_b));
    let body = axum::body::Body::from_stream(tokio_util::io::ReaderStream::new(file));
    Ok((
        [
            (
                axum::http::header::CONTENT_TYPE,
                "text/csv; charset=utf-8".to_string(),
            ),
            (
                axum::http::header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{name}\""),
            ),
        ],
        body,
    )
        .into_response())
}

/// Forget a diff and delete its uploaded files and results.
async fn remove(
    State(app): State<Arc<App>>,
    UrlPath(id): UrlPath<u64>,
) -> Result<StatusCode, ApiError> {
    let job = find(&app, id)?;
    if matches!(*job.state.lock().unwrap(), JobState::Running) {
        return Err(ApiError(
            StatusCode::CONFLICT,
            "wait for this diff to finish before deleting it".into(),
        ));
    }
    app.jobs.lock().unwrap().remove(&id);
    let dir = app.dir.path().join(id.to_string());
    tokio::fs::remove_dir_all(dir).await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let state = Arc::new(App {
        dir: tempfile::Builder::new().prefix("rowdiff-web-").tempdir()?,
        next: AtomicU64::new(1),
        jobs: Mutex::new(HashMap::new()),
    });
    let app = Router::new()
        .route("/api/health", get(|| async { "ok" }))
        .route("/api/diffs", post(create))
        .route("/api/diffs/{id}", get(status).delete(remove))
        .route("/api/diffs/{id}/changes.csv", get(export))
        .route("/api/diffs/{id}/rows", get(rows))
        .layer(DefaultBodyLimit::max(cli.max_upload_mb << 20))
        .with_state(state);
    let app = match &cli.static_dir {
        Some(dir) => app
            .fallback_service(ServeDir::new(dir).fallback(ServeFile::new(dir.join("index.html")))),
        None => app.fallback(embedded_ui),
    };

    let listener = tokio::net::TcpListener::bind(cli.addr).await?;
    println!("rowdiff is running at http://{}", cli.addr);
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

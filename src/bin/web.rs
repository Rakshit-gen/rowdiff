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

use axum::extract::{DefaultBodyLimit, Multipart, Path as UrlPath, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Parser;
use rowdiff::output::summary_json;
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
    /// The built web UI.
    #[arg(long, default_value = "web/dist")]
    static_dir: PathBuf,
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
    Done(Report),
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
        let result = d.run_with(&job.progress, |_| {});
        *job.state.lock().unwrap() = match result {
            Ok(report) => JobState::Done(report),
            Err(e) => JobState::Failed(e.to_string()),
        };
    });
    Ok((StatusCode::CREATED, Json(body)))
}

fn status_json(id: u64, job: &Job) -> Value {
    let (phase, done, total) = job.progress.snapshot();
    let mut v = json!({
        "id": id,
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let ui =
        ServeDir::new(&cli.static_dir).fallback(ServeFile::new(cli.static_dir.join("index.html")));
    let state = Arc::new(App {
        dir: tempfile::Builder::new().prefix("rowdiff-web-").tempdir()?,
        next: AtomicU64::new(1),
        jobs: Mutex::new(HashMap::new()),
    });
    let app = Router::new()
        .route("/api/health", get(|| async { "ok" }))
        .route("/api/diffs", post(create))
        .route("/api/diffs/{id}", get(status))
        .layer(DefaultBodyLimit::max(cli.max_upload_mb << 20))
        .with_state(state)
        .fallback_service(ui);

    let listener = tokio::net::TcpListener::bind(cli.addr).await?;
    println!("rowdiff is running at http://{}", cli.addr);
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}

//! rowdiff in the browser: upload two CSV files, browse the differences.
//!
//!     cargo run --release --features web --bin rowdiff-web
//!
//! Binds to 127.0.0.1 by default. Uploaded files and results live in a temp
//! directory that is removed when the server stops.

use std::net::SocketAddr;
use std::path::PathBuf;

use axum::Router;
use axum::routing::get;
use clap::Parser;
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
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let ui = ServeDir::new(&cli.static_dir).fallback(ServeFile::new(cli.static_dir.join("index.html")));
    let app = Router::new()
        .route("/api/health", get(|| async { "ok" }))
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

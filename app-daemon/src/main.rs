//! app-daemon: a headless rqbit daemon that speaks the `rqbit-protocol`
//! WebSocket protocol (postcard frames over `GET /ws`), backed directly by
//! the `librqbit::Api` Rust layer. UI clients (the browser wasm build of
//! rqbit-ui) connect to this process.

use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::Context as _;
use clap::Parser;

use app_daemon::server;
use app_daemon::start_session;

#[derive(Parser)]
#[command(
    name = "app-daemon",
    about = "Headless rqbit daemon for rqbit UI clients"
)]
struct Args {
    /// Folder where downloaded data is stored.
    #[arg(long, default_value = "downloads")]
    download_folder: PathBuf,

    /// HTTP + WebSocket listen address. Serves the embedded web client at
    /// / (the port and path the classic rqbit webui uses) and the
    /// protocol at /ws.
    #[arg(long, default_value = "[::]:3080")]
    listen: SocketAddr,

    /// Enable JSON persistence in this folder (fastresume across restarts).
    #[arg(long)]
    persistence_folder: Option<PathBuf>,

    /// Disable the DHT (mostly for tests and offline use).
    #[arg(long)]
    disable_dht: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Logging: the returned broadcast carries every formatted log line; the
    // WS server forwards it to clients that subscribe to logs.
    let init_logging = librqbit::tracing_subscriber_config_utils::init_logging(
        librqbit::tracing_subscriber_config_utils::InitLoggingOptions {
            default_rust_log_value: Some("info"),
            log_file: None,
            log_file_rust_log: None,
            log_file_json: false,
        },
    )?;

    let daemon = start_session(
        args.download_folder,
        args.persistence_folder,
        args.disable_dht,
        init_logging,
    )
    .await?;

    let app = server::router(daemon.api.clone(), daemon.line_broadcast.clone());

    let listener = tokio::net::TcpListener::bind(args.listen)
        .await
        .with_context(|| format!("error listening on {}", args.listen))?;
    tracing::info!(addr = %args.listen, "app-daemon listening");
    axum::serve(listener, app).await?;
    Ok(())
}

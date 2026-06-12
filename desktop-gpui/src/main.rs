// desktop-gpui/src/main.rs

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;

use std::{
    fs::{File, OpenOptions},
    io::BufReader,
    path::Path,
    sync::Arc,
};

use anyhow::Context;
use config::RqbitDesktopConfig;
use http::StatusCode;
use librqbit::{
    AddTorrent, AddTorrentOptions, Api, ApiError, DhtSessionConfig,
    Session, SessionOptions, SessionPersistenceConfig, WithStatusError,
    api::{
        ApiAddTorrentResponse, ApiTorrentListOpts, EmptyJsonResponse,
        TorrentDetailsResponse, TorrentIdOrHash, TorrentListResponse,
        TorrentStats,
    },
    dht::DhtPersistenceConfig,
    http_api_types::{PeerStatsFilter, PeerStatsSnapshot},
    session_stats::snapshot::SessionStatsSnapshot,
    tracing_subscriber_config_utils::{
        InitLoggingOptions, InitLoggingResult, init_logging,
    },
};
use librqbit_dualstack_sockets::TcpListener;
use parking_lot::RwLock;
use serde::Serialize;
use tracing::{debug_span, error, info, warn};

use gpui::{
    App, Application, Bounds, SharedString, Window,
    WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
};

use crate::state::{SharedState};
use ipc::IpcService;

/// Simple placeholder component – replace with the real UI later.
pub struct HelloWorld {
    text: SharedString,
}

impl Render for HelloWorld {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div().child(format!("Hello, {}!", &self.text))
    }
}

/// Load the config file and create a librqbit `Api`.
async fn api_from_config(
    init_logging: &InitLoggingResult,
    config: RqbitDesktopConfig,
) -> anyhow::Result<Api> {
    // Validate configuration first
    config.validate().context("error validating configuration")?;

    /* ---------- Persistence ---------------------------------------- */
    let persistence = if config.persistence.disable {
        None
    } else {
        Some(SessionPersistenceConfig::Json {
            folder: if config.persistence.folder == Path::new("") {
                None
            } else {
                Some(config.persistence.folder.clone())
            },
        })
    };

    /* ---------- Listener / connect options ------------------------ */
    let (listen, connect) = config.connections.as_listener_and_connect_opts();

    /* ---------- HTTP API options ----------------------------------- */
    let mut http_api_opts = librqbit::http_api::HttpApiOptions {
        read_only: config.http_api.read_only,
        basic_auth: None,
        ..Default::default()
    };

    if !config.http_api.disable {
        match metrics_exporter_prometheus::PrometheusBuilder::new().install_recorder() {
            Ok(handle) => http_api_opts.prometheus_handle = Some(handle),
            Err(e) => warn!("error installing prometheus recorder: {e:#}"),
        }
    }

    /* ---------- DHT ----------------------------------------------- */
    let dht = if config.dht.disable {
        None
    } else {
        let persistence = if config.dht.disable_persistence {
            None
        } else {
            Some(DhtPersistenceConfig {
                config_filename: Some(config.dht.persistence_filename.clone()),
                ..Default::default()
            })
        };
        Some(DhtSessionConfig {
            persistence,
            ..Default::default()
        })
    };

    /* ---------- Session -------------------------------------------- */
    let session = Session::new_with_opts(
        config.default_download_location.clone(),
        SessionOptions {
            dht,
            persistence,
            connect: Some(connect),
            listen,
            fastresume: config.persistence.fastresume,
            ratelimits: config.ratelimits,
            #[cfg(feature = "disable-upload")]
            disable_upload: config.disable_upload,
            ..Default::default()
        },
    )
    .await
    .context("couldn't set up librqbit session")?;

    /* ---------- API ---------------------------------------------- */
    let api = Api::new(
        session.clone(),
        Some(init_logging.rust_log_reload_tx.clone()),
        Some(init_logging.line_broadcast.clone()),
    );

    /* ---------- HTTP API server ----------------------------------- */
    if !config.http_api.disable {
        let listen_addr = config.http_api.listen_addr;
        let api_clone = api.clone();
        let upnp_router = if config.upnp.enable_server {
            // Friendly name for the UPnP server
            let friendly_name = config
                .upnp
                .server_friendly_name
                .as_ref()
                .map(|f| f.trim())
                .filter(|s| !s.is_empty())
                .map(String::to_owned)
                .unwrap_or_else(|| {
                    format!(
                        "rqbit-desktop@{}",
                        gethostname::gethostname().to_string_lossy()
                    )
                });

            // Start the UPnP SSDP router
            let mut upnp_adapter = session
                .make_upnp_adapter(friendly_name, config.http_api.listen_addr.port())
                .await
                .context("error starting UPnP server")?;
            let router = upnp_adapter.take_router()?;

            session.spawn(debug_span!("ssdp"), "ssdp", async move {
                upnp_adapter.run_ssdp_forever().await
            });

            Some(router)
        } else {
            None
        };

        // Run the HTTP API in its own task
        let http_api_task = async move {
            let listener =
                TcpListener::bind_tcp(listen_addr, Default::default()).with_context(|| {
                    format!("error listening on {}", listen_addr)
                })?;
            librqbit::http_api::HttpApi::new(api_clone, Some(http_api_opts))
                .make_http_api_and_run(listener, upnp_router)
                .await
        };

        session.spawn(debug_span!("http_api"), "http_api", http_api_task);
    }

    Ok(api)
}

/// Create the shared state that GPUI will use.
async fn init_shared_state(init_logging: InitLoggingResult) -> anyhow::Result<SharedState> {
    // Configuration file path
    let config_path = directories::ProjectDirs::from("com", "rqbit", "desktop")
        .expect("directories::ProjectDirs::from")
        .config_dir()
        .join("config.json");

    // Load the configuration
    let config: RqbitDesktopConfig = {
        let rdr = BufReader::new(File::open(&config_path)?);
        let mut cfg: RqbitDesktopConfig = serde_json::from_reader(rdr)?;
        cfg.persistence.fix_backwards_compat();
        cfg
    };

    // Create the session + IPC service
    let api = api_from_config(&init_logging, config.clone()).await?;
    Ok(SharedState {
        config,
        ipc: Arc::new(IpcService::new(api)),
    })
}

#[tokio::main]
async fn main() {
    /* ---------- Logging ------------------------------------------------- */
    let init_logging_result = init_logging(InitLoggingOptions {
        default_rust_log_value: Some("info"),
        log_file: None,
        log_file_rust_log: None,
        log_file_json: false,
    })
    .expect("failed to initialise logging");

    /* ---------- File‑descriptor limit --------------------------------- */
    match librqbit::try_increase_nofile_limit() {
        Ok(limit) => info!(limit = limit, "increased open file limit"),
        Err(e) => warn!("failed increasing open file limit: {:#}", e),
    }

    /* ---------- Shared state ------------------------------------------- */
    let shared_state = init_shared_state(init_logging_result)
        .await
        .expect("failed to initialise shared state");

    info!("GPUI application started – state ready");

    /* ---------- GPUI --------------------------------------------------- */
    Application::new().run(|cx: &mut App| {
        // Inject the shared state into GPUI
        cx.set_context(shared_state.clone());

        let bounds = Bounds::centered(None, size(px(700.), px(500.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| HelloWorld { text: "World".into() })
            },
        )
        .unwrap();
    });
}
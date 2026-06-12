// Full replacement of main.rs with simplified GPUI app that initialises session and IPC
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;

use std::sync::Arc;

use anyhow::Context;
use config::RqbitDesktopConfig;
use http::StatusCode;
use librqbit::{
    AddTorrent, AddTorrentOptions, Api, ApiError, DhtSessionConfig, Session, SessionOptions,
    SessionPersistenceConfig, WithStatusError,
    api::{
        ApiAddTorrentResponse, ApiTorrentListOpts, EmptyJsonResponse, TorrentDetailsResponse,
        TorrentIdOrHash, TorrentListResponse, TorrentStats,
    },
    dht::DhtPersistenceConfig,
    http_api_types::{PeerStatsFilter, PeerStatsSnapshot},
    session_stats::snapshot::SessionStatsSnapshot,
    tracing_subscriber_config_utils::{InitLoggingOptions, InitLoggingResult, init_logging},
};
use librqbit_dualstack_sockets::TcpListener;
use parking_lot::RwLock;
use serde::Serialize;
use tracing::{debug_span, error, info, warn};

use gpui::{
    App, Application, Bounds, SharedString, Window, WindowBounds, WindowOptions, div,
    prelude::*, px, rgb, size,
};

use crate::state::{SharedState, init_shared_state};
use ipc::IpcService;

/// A simple placeholder component for now.
pub struct HelloWorld {
    text: SharedString,
}

impl Render for HelloWorld {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div().child(format!("Hello, {}!", &self.text))
    }
}

/// Initialise the librqbit session and return a SharedState.
async fn init_shared_state(init_logging: InitLoggingResult) -> anyhow::Result<SharedState> {
    let config_path = directories::ProjectDirs::from("com", "rqbit", "desktop")
        .expect("directories::ProjectDirs::from")
        .config_dir()
        .join("config.json");

    let config: RqbitDesktopConfig = {
        let rdr = std::fs::File::open(&config_path)?;
        let mut cfg: RqbitDesktopConfig = serde_json::from_reader(rdr)?;
        cfg.persistence.fix_backwards_compat();
        cfg
    };

    let api = api_from_config(&init_logging, &config).await?;
    Ok(SharedState {
        config,
        ipc: Arc::new(IpcService::new(api)),
    })
}

#[tokio::main]
async fn main() {
    // Initialise logging – same options as the Tauri binary.
    let init_logging_result = init_logging(InitLoggingOptions {
        default_rust_log_value: Some("info"),
        log_file: None,
        log_file_rust_log: None,
        log_file_json: false,
    })
    .expect("failed to initialise logging");

    // Increase file descriptor limit, mirroring the Tauri behaviour.
    match librqbit::try_increase_nofile_limit() {
        Ok(limit) => info!(limit = limit, "increased open file limit"),
        Err(e) => warn!("failed increasing open file limit: {:#}", e),
    }

    // Initialise shared state.
    let shared_state = init_shared_state(init_logging_result).await.expect("failed to initialise state");

    info!("GPUI application started – state ready");

    // Run the GPUI application.
    Application::new().run(|cx: &mut App| {
        // Provide the shared state to the GPUI context.
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

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
    App, AppContext, Application, Bounds, SharedString, Window, WindowBounds, WindowOptions, div,
    prelude::*, px, rgb, size,
};

// mod http_api;
// use crate::http_api::HttpClient;

/// Simple placeholder component – replace with the real UI later.
pub struct HelloWorld {
    text: SharedString,
}

impl Render for HelloWorld {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div().child(format!("Hello, {}!", &self.text))
    }
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

    info!("GPUI application started – state ready");

    /* ---------- GPUI --------------------------------------------------- */
    Application::new().run(|cx: &mut App| {
        // Inject the shared state into GPUI
        // cx.set_global(shared_state.clone());

        let bounds = Bounds::centered(None, size(px(700.), px(500.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| HelloWorld {
                    text: "World".into(),
                })
            },
        )
        .unwrap();
    });
}

// desktop‑gpui/src/main.rs

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
pub mod http_api;
mod state; // shared_state

use std::sync::Arc;

use config::RqbitDesktopConfig;
use gpui::{
    App, Application, Bounds, SharedString, Window, WindowBounds, WindowOptions, div, prelude::*,
    px, size,
};

mod ui;

use crate::state::SharedState;

/// Placeholder until MainPanel is wired
pub struct HelloWorld {
    text: SharedString,
}

impl Render for HelloWorld {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div().child(format!("Hello, {}!", &self.text))
    }
}

async fn init_shared_state(
    init_logging: crate::tracing_subscriber_config_utils::InitLoggingResult,
) -> anyhow::Result<SharedState> {
    // Load config
    let config_path = directories::ProjectDirs::from("com", "rqbit", "desktop")
        .expect("directories::ProjectDirs::from")
        .config_dir()
        .join("config.json");
    let config: RqbitDesktopConfig = {
        let rdr = std::io::BufReader::new(std::fs::File::open(&config_path)?);
        let mut cfg: RqbitDesktopConfig = serde_json::from_reader(rdr)?;
        cfg.persistence.fix_backwards_compat();
        cfg
    };

    // HTTP client
    let http_client = Arc::new(crate::http_api::HttpClient::new(
        "http://127.0.0.1:3030".to_string(),
    ));

    Ok(SharedState {
        config,
        http_client,
    })
}

#[tokio::main]
async fn main() {
    // Logging
    let init_logging_result = crate::tracing_subscriber_config_utils::init_logging(
        crate::tracing_subscriber_config_utils::InitLoggingOptions {
            default_rust_log_value: Some("info"),
            log_file: None,
            log_file_rust_log: None,
            log_file_json: false,
        },
    )
    .expect("failed to initialise logging");

    // File‑descriptor limit
    if let Ok(limit) = librqbit::try_increase_nofile_limit() {
        info!(limit = limit, "increased open file limit");
    }

    // Shared state
    let shared_state = init_shared_state(init_logging_result)
        .await
        .expect("failed to initialise shared state");

    info!("GPUI application started – state ready");

    // Run GPUI
    Application::new().run(|cx: &mut App| {
        cx.set_global(shared_state.clone());

        let bounds = Bounds::centered(None, size(px(700.), px(500.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| {
                // Replace with MainPanel when ready
                cx.new(|_| MainPanel::default())
            },
        )
        .unwrap();
    });
}

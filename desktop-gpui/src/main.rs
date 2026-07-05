#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use gpui::*;
use tracing::{info, warn};

mod config;
mod ui;

mod state;
use crate::{state::State, ui::main_panel::MainPanel};
impl gpui::Global for State {}

use librqbit::tracing_subscriber_config_utils::{InitLoggingOptions, init_logging};

#[tokio::main]
async fn main() {
    // Logging
    let init_logging_result = init_logging(InitLoggingOptions {
        default_rust_log_value: Some("info"),
        log_file: None,
        log_file_rust_log: None,
        log_file_json: false,
    })
    .unwrap();

    match librqbit::try_increase_nofile_limit() {
        Ok(limit) => info!(limit = limit, "increased open file limit"),
        Err(e) => warn!("failed increasing open file limit: {:#}", e),
    };

    // Shared state
    let shared_state = State::new(init_logging_result)
        .await
        .expect("failed to create state");

    info!("GPUI application started – state ready");

    // Run GPUI
    let platform = gpui_platform::application();
    platform.run(move |cx: &mut App| {
        // Store the state globally so all windows/views can access it
        cx.set_global(shared_state.clone());

        gpui_component::init(cx);

        let window_options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("rqbit".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(13.), px(13.))),
            }),
            window_bounds: Some(WindowBounds::centered(size(px(1000.), px(800.)), cx)),
            window_decorations: Some(WindowDecorations::Server),
            window_min_size: Some(size(px(600.), px(400.))),
            ..Default::default()
        };

        let window = cx
            .open_window(window_options, |window, cx| {
                let main_panel = cx.new(|cx| MainPanel::new(window, cx));
                cx.new(|cx| gpui_component::Root::new(main_panel, window, cx))
            })
            .expect("Failed to open window");

        // Quit the app when the window is closed (native close button).
        cx.on_window_closed(move |cx: &mut App, _window_id| {
            if window.is_active(cx).unwrap_or(false) {
                cx.quit();
            }
        })
        .detach();
    });
}

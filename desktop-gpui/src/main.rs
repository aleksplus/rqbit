#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use gpui::{App, Application, Bounds, WindowBounds, WindowOptions, prelude::*, px, size};
use gpui_component::Root;
use tracing::{info, warn};

mod config;
mod ui;

mod state;
use crate::{
    state::{State, StateShared},
    ui::main_panel,
};
impl gpui::Global for state::State {}

use librqbit::{
    api::ApiTorrentListOpts,
    tracing_subscriber_config_utils::{InitLoggingOptions, init_logging},
};

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
    let shared_state = State::new(init_logging_result).await;

    info!("GPUI application started – state ready");

    // Run GPUI
    Application::new().run(move |cx: &mut App| {
        // cx.set_global(shared_state);

        gpui_component::init(cx);

        // let bounds = Bounds::centered(None, size(px(700.), px(500.)), cx);

        // fn load_initial_data(&mut self, cx: &mut Context<Self>) {
        //     // Spawn a background task managed by GPUI's built-in executor
        //     cx.spawn(|this, mut cx| async move {
        //         // 1. Do your heavy async work here (API calls, file reads, etc.)
        //         let fetched_text = fake_api_call().await;

        //         // 2. Safe bridge back to the UI thread to update your state
        //         let _ = this.update(&mut cx, |view, cx| {
        //             view.data = Some(fetched_text);

        //             // 3. Tell GPUI that the data changed and it needs to render again
        //             cx.notify();
        //         });
        //     })
        //     .detach(); // Detach lets the task run independently in the background
        // }
        // let client = shared_state
        //     .api()?
        //     .api_torrent_list_ext(ApiTorrentListOpts { with_stats: true });
        // // Demo: block on the future; in real code use cx.spawn.
        // self.torrents = client.list_torrents(true).await.unwrap().torrents; // `block_on` is only for demonstration

        cx.spawn(async move |cx| {
            // let _ = cx.update(|cx| {
            //     // cx.view.data = Some(fetched_text);

            //     // 3. Tell GPUI that the data changed and it needs to render again
            //     cx.notify();
            // });
            cx.open_window(WindowOptions::default(), |window, cx| {
                let view = cx.new(|_| ui::main_panel::MainPanel);
                // This first level on the window, should be a Root.
                cx.new(|cx| Root::new(view, window, cx))
            })
            .expect("Failed to open window");

            // cx.open_window(
            //     WindowOptions {
            //         window_bounds: Some(WindowBounds::Windowed(bounds)),
            //         ..Default::default()
            //     },
            //     |_, cx| cx.new(|_| ui::main_panel::MainPanel::new()),
            // )
            // .unwrap();
        })
        .detach();

        // cx.open_window(
        //     WindowOptions {
        //         window_bounds: Some(WindowBounds::Windowed(bounds)),
        //         ..Default::default()
        //     },
        //     |_, cx| cx.new(|_| ui::main_panel::MainPanel::default()),
        // )
        // .unwrap();
    });
}

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// use gpui::{
//     App, Application, Bounds, Window, WindowBounds, WindowDecorations, WindowOptions, div, prelude::*, px, size
// };
use gpui::*;
use gpui_component::{ActiveTheme as _, Root, StyledExt as _, h_flex, v_flex};
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

struct RootBorderlessExample;

impl Render for RootBorderlessExample {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .gap_4()
            .p_8()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                div()
                    .text_2xl()
                    .font_semibold()
                    .child("Root::bordered(false)"),
            )
            .child(
                div()
                    .max_w(px(560.))
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        "This window requests client-side decorations, while Root disables GPUI Component's window border wrapper.",
                    ),
            )
            .child(
                h_flex()
                    .gap_3()
                    .child(
                        div()
                            .rounded_md()
                            .border_1()
                            .border_color(cx.theme().border)
                            .px_3()
                            .py_2()
                            .child("Root.bordered = false"),
                    )
                    .child(
                        div()
                            .rounded_md()
                            .border_1()
                            .border_color(cx.theme().border)
                            .px_3()
                            .py_2()
                            .child("window_decorations = Client"),
                    ),
            )
    }
}

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

        gpui_platform::application().run(move |cx| {
            gpui_component::init(cx);

            let window_options = WindowOptions {
                titlebar: None,
                window_bounds: Some(WindowBounds::centered(size(px(640.), px(320.)), cx)),
                window_decorations: Some(WindowDecorations::Client),
                ..Default::default()
            };

            cx.spawn(async move |cx| {
                cx.open_window(window_options, |window, cx| {
                    let view = cx.new(|cx| RootBorderlessExample);
                    view
                })
                .expect("Failed to open window");
            })
            .detach();
        });
    });
}

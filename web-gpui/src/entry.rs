//! wasm entrypoint: initialize the web platform, connect to the daemon,
//! open the window with the shared MainPanel.

use std::sync::Arc;

use gpui::{App, AppContext as _, TitlebarOptions, WindowOptions};
use rqbit_ui::ui::main_panel::MainPanel;
use wasm_bindgen::prelude::*;

use crate::remote_client::RemoteClient;
use crate::web_host::WebHost;

/// Boot the UI. `daemon_url` is the `ws(s)://host:port/ws` endpoint of an
/// `app-daemon` process. The JS shell calls this after the wasm module
/// initializes; the GPUI event loop keeps running from the returned closure.
#[wasm_bindgen]
pub fn run(daemon_url: String) -> Result<(), JsValue> {
    // Panic hooks + console logging for the web platform.
    gpui_platform::web_init();

    let client = Arc::new(RemoteClient::new(daemon_url));
    let host = Arc::new(WebHost);

    let app = gpui_platform::single_threaded_web();
    app.run(move |cx: &mut App| {
        gpui_kit::init(cx);

        // Dark theme by default; toggling persists for the session only.
        gpui_kit::component::Theme::change(
            gpui_kit::component::ThemeMode::Dark,
            None,
            cx,
        );

        let window_options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("rqbit".into()),
                ..Default::default()
            }),
            window_min_size: Some(gpui::size(gpui::px(600.), gpui::px(400.))),
            ..Default::default()
        };

        gpui_kit::open_window(window_options, cx, |window, cx| {
            cx.new(|cx| MainPanel::new(client.clone(), host.clone(), window, cx))
        })
        .expect("failed to open window");
    });

    // `Application::run` returns after handing control to the browser event
    // loop; nothing to keep alive on the Rust side.
    Ok(())
}

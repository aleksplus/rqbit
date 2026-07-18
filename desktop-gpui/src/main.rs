#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use gpui::*;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tracing::{info, warn};

mod config;
mod ui;

mod state;
use crate::{state::State, ui::main_panel::MainPanel};
impl gpui::Global for State {}

use librqbit::tracing_subscriber_config_utils::{InitLoggingOptions, init_logging};

/// Root view that wraps the main panel and implements "hold-to-quit":
/// keep cmd-q / ctrl-c held for 1 second to exit. Releasing either the key or
/// the modifier early drops the countdown task, cancelling the quit. The key
/// listeners are attached to this element, so they are registered during
/// paint (safe) and cleared each frame.
struct HoldToQuitView {
    main_panel: Entity<MainPanel>,
    quit_countdown: Arc<Mutex<Option<Task<()>>>>,
}

impl Render for HoldToQuitView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let quit_countdown = self.quit_countdown.clone();
        div()
            .size_full()
            .on_key_down({
                let quit_countdown = quit_countdown.clone();
                move |event: &KeyDownEvent, _window, cx| {
                    // The gesture is "hold Cmd-Q / Ctrl-C": require the
                    // secondary modifier (Cmd on macOS, Ctrl elsewhere) plus q or c.
                    if !event.keystroke.modifiers.secondary()
                        || (event.keystroke.key != "q" && event.keystroke.key != "c")
                    {
                        return;
                    }
                    let mut lock = quit_countdown.lock().unwrap();
                    if lock.is_some() {
                        return; // already counting down (OS key-repeat guard)
                    }
                    info!("Hotkey held down. Keep holding for 1 second to exit…");
                    let executor = cx.background_executor().clone();
                    let task = cx.spawn(move |cx: &mut AsyncApp| {
                        let async_cx = cx.clone();
                        let executor = executor.clone();
                        async move {
                            executor.timer(Duration::from_secs(1)).await;
                            async_cx.update(|cx| {
                                info!("1 second elapsed! Exiting…");
                                cx.quit();
                            });
                        }
                    });
                    *lock = Some(task);
                }
            })
            .on_key_up({
                let quit_countdown = quit_countdown.clone();
                move |event: &KeyUpEvent, _window, _cx| {
                    // Releasing the letter key (q/c) cancels the pending quit.
                    if event.keystroke.key != "q" && event.keystroke.key != "c" {
                        return;
                    }
                    let mut lock = quit_countdown.lock().unwrap();
                    if lock.is_some() {
                        info!("Key released early! Cancellation successful.");
                        *lock = None; // dropping the task aborts the timer
                    }
                }
            })
            .on_modifiers_changed({
                let quit_countdown = quit_countdown.clone();
                move |event: &ModifiersChangedEvent, _window, _cx| {
                    // Releasing the modifier (Cmd/Ctrl) first also breaks the
                    // gesture and must cancel the pending quit.
                    if event.modifiers.secondary() {
                        return;
                    }
                    let mut lock = quit_countdown.lock().unwrap();
                    if lock.is_some() {
                        info!("Modifier released early! Cancellation successful.");
                        *lock = None; // dropping the task aborts the timer
                    }
                }
            })
            .child(self.main_panel.clone())
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
    let shared_state = State::new(init_logging_result)
        .await
        .expect("failed to create state");

    info!("GPUI application started – state ready");

    // Run GPUI
    let platform = gpui_platform::application().with_assets(gpui_component_assets::Assets);
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

        // Hold-to-quit: keep cmd-q / ctrl-c held for 1 second to exit. Releasing
        // either the key or the modifier early drops the countdown task,
        // cancelling the quit. The key listeners are attached to the root
        // element, so they are registered during paint (safe) and cleared each
        // frame.
        let quit_countdown: Arc<Mutex<Option<Task<()>>>> = Arc::new(Mutex::new(None));

        let window = cx
            .open_window(window_options, |window, cx| {
                let main_panel = cx.new(|cx| MainPanel::new(window, cx));
                let root_view = cx.new(|_cx| HoldToQuitView {
                    main_panel,
                    quit_countdown: quit_countdown.clone(),
                });
                cx.new(|cx| gpui_component::Root::new(root_view, window, cx))
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

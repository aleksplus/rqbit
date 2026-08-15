use gpui::*;
use gpui_component::{
    ActiveTheme as _, Disableable as _, Sizable as _, Size,
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputEvent, InputState},
    scroll::ScrollableElement,
    setting::{RenderOptions, SettingField, SettingGroup, SettingItem, SettingPage, Settings},
    v_flex,
};
use std::{num::NonZeroU32, sync::Arc};

use crate::{
    config::{RqbitDesktopConfig, write_config},
    state::State,
};

type ConfigArc = Arc<parking_lot::RwLock<RqbitDesktopConfig>>;

/// Events emitted by [`SettingsPage`] to communicate with the parent view.
#[derive(Clone, Debug)]
pub enum SettingsPageEvent {
    /// User clicked **Back** – close the settings page.
    Back,
}

/// A full-page settings view built with the gpui-component `Settings` component.
///
/// Replaces the old `ConfigModal`. The config is stored in an `Arc<RwLock<...>>`
/// so that the `SettingField` closures (which receive `&App` / `&mut App`) can
/// read and mutate it.  An **Apply** button persists the config and reconfigures
/// the live session.
pub struct SettingsPage {
    state: Arc<State>,
    config: ConfigArc,
    focus_handle: FocusHandle,
}

impl EventEmitter<SettingsPageEvent> for SettingsPage {}

impl SettingsPage {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>, state: Arc<State>) -> Self {
        let config = state.shared().read().config();
        Self {
            state,
            config: Arc::new(parking_lot::RwLock::new(config)),
            focus_handle: cx.focus_handle(),
        }
    }

    fn on_back(&mut self, cx: &mut Context<Self>) {
        cx.emit(SettingsPageEvent::Back);
    }

    fn on_apply(&mut self, cx: &mut Context<Self>) {
        let config = self.config.read().clone();

        if let Err(e) = write_config(&self.state.config_filename, &config) {
            eprintln!("Error writing config: {:?}", e);
        }

        let state = self.state.clone();
        let entity_id = cx.entity_id();
        cx.spawn(async move |_, cx| {
            if let Err(e) = state.configure(config).await {
                eprintln!("Error reconfiguring session: {:?}", e);
            }
            cx.update(|cx| cx.notify(entity_id));
        })
        .detach();

        cx.emit(SettingsPageEvent::Back);
    }

    fn on_reset(&mut self, cx: &mut Context<Self>) {
        *self.config.write() = RqbitDesktopConfig::default();
        cx.notify();
    }
}

// ── Field helpers ──────────────────────────────────────────────────────────
// Each helper clones the ConfigArc so the `value` and `set_value` closures
// each own their own Arc (avoiding "borrow of moved value" errors).

fn switch_field(
    config: ConfigArc,
    get: fn(&RqbitDesktopConfig) -> bool,
    set: fn(&mut RqbitDesktopConfig, bool),
) -> SettingField<bool> {
    let cfg_get = config.clone();
    let cfg_set = config;
    SettingField::switch(
        move |_| get(&cfg_get.read()),
        move |val: bool, _| set(&mut cfg_set.write(), val),
    )
}

fn input_field(
    config: ConfigArc,
    get: fn(&RqbitDesktopConfig) -> String,
    set: fn(&mut RqbitDesktopConfig, SharedString),
) -> SettingField<SharedString> {
    let cfg_get = config.clone();
    let cfg_set = config;
    SettingField::input(
        move |_| get(&cfg_get.read()).into(),
        move |val: SharedString, _| set(&mut cfg_set.write(), val),
    )
}

/// State backing the download-location field: the text input plus the
/// subscription that writes typed changes back into the config.
struct DlInputState {
    input: Entity<InputState>,
    _subscription: gpui::Subscription,
}

/// A download-location field that pairs a text input with a **Browse…** button.
///
/// The button opens the native folder picker (macOS Finder / open panel).
/// On macOS this also triggers the OS "grant access to this folder" prompt the
/// first time, and the granted access persists for the app — so the user is
/// only asked once.
fn download_location_field(
    config: ConfigArc,
) -> impl Fn(&RenderOptions, &mut Window, &mut App) -> AnyElement {
    move |options, window, cx| {
        let value = get_dl_location(&config.read());
        let set = set_dl_location;
        let cfg = config.clone();

        let state_entity = window.use_keyed_state("download-location-input", cx, {
            let value = value.clone();
            move |window, cx| {
                let input = cx.new(|cx| InputState::new(window, cx).default_value(value.clone()));
                let _subscription = cx.subscribe(&input, {
                    let cfg = cfg.clone();
                    move |_, input, event: &InputEvent, cx| {
                        if matches!(event, InputEvent::Change) {
                            let v = input.read(cx).value();
                            set(&mut cfg.write(), v);
                        }
                    }
                });
                DlInputState {
                    input,
                    _subscription,
                }
            }
        });

        // Keep the displayed text in sync when the config changes externally
        // (e.g. after picking a folder via the Browse button).
        state_entity.update(cx, |state, cx| {
            if state.input.read(cx).value() != value {
                state.input.update(cx, |input, cx| {
                    input.set_value(value.clone(), window, cx);
                });
            }
        });

        let state = state_entity.read(cx);

        h_flex()
            .gap_2()
            .child(
                Input::new(&state.input)
                    .flex_1()
                    .disabled(options.disabled)
                    .with_size(options.size),
            )
            .child(
                Button::new("browse-download-folder")
                    .label("Browse…")
                    .disabled(options.disabled)
                    .on_click({
                        let cfg = config.clone();
                        move |_, _window, cx| {
                            let rx = cx.prompt_for_paths(PathPromptOptions {
                                files: false,
                                directories: true,
                                multiple: false,
                                prompt: Some("Select download folder".into()),
                            });
                            let cfg = cfg.clone();
                            cx.spawn(async move |cx| {
                                if let Ok(Ok(Some(paths))) = rx.await
                                    && let Some(path) = paths.into_iter().next()
                                {
                                    set(
                                        &mut cfg.write(),
                                        path.to_string_lossy().into_owned().into(),
                                    );
                                    cx.update(|cx| cx.refresh_windows());
                                }
                            })
                            .detach();
                        }
                    }),
            )
            .into_any_element()
    }
}

// ── Config accessors ────────────────────────────────────────────────────────

fn get_dl_location(c: &RqbitDesktopConfig) -> String {
    c.default_download_location.to_string_lossy().to_string()
}
fn set_dl_location(c: &mut RqbitDesktopConfig, v: SharedString) {
    c.default_download_location = v.to_string().into();
}

fn get_dht_disable(c: &RqbitDesktopConfig) -> bool {
    !c.dht.disable
}
fn set_dht_disable(c: &mut RqbitDesktopConfig, v: bool) {
    c.dht.disable = !v;
}

fn get_dht_persist(c: &RqbitDesktopConfig) -> bool {
    !c.dht.disable_persistence
}
fn set_dht_persist(c: &mut RqbitDesktopConfig, v: bool) {
    c.dht.disable_persistence = !v;
}

fn get_dht_filename(c: &RqbitDesktopConfig) -> String {
    c.dht.persistence_filename.to_string_lossy().to_string()
}
fn set_dht_filename(c: &mut RqbitDesktopConfig, v: SharedString) {
    c.dht.persistence_filename = v.to_string().into();
}

fn get_persist_disable(c: &RqbitDesktopConfig) -> bool {
    !c.persistence.disable
}
fn set_persist_disable(c: &mut RqbitDesktopConfig, v: bool) {
    c.persistence.disable = !v;
}

fn get_persist_folder(c: &RqbitDesktopConfig) -> String {
    c.persistence.folder.to_string_lossy().to_string()
}
fn set_persist_folder(c: &mut RqbitDesktopConfig, v: SharedString) {
    c.persistence.folder = v.to_string().into();
}

fn get_fastresume(c: &RqbitDesktopConfig) -> bool {
    c.persistence.fastresume
}
fn set_fastresume(c: &mut RqbitDesktopConfig, v: bool) {
    c.persistence.fastresume = v;
}

fn get_dl_rate(c: &RqbitDesktopConfig) -> String {
    c.ratelimits
        .download_bps
        .map(|v| v.to_string())
        .unwrap_or_default()
}
fn set_dl_rate(c: &mut RqbitDesktopConfig, v: SharedString) {
    c.ratelimits.download_bps = v
        .parse::<u32>()
        .ok()
        .filter(|&v| v > 0)
        .and_then(NonZeroU32::new);
}

fn get_ul_rate(c: &RqbitDesktopConfig) -> String {
    c.ratelimits
        .upload_bps
        .map(|v| v.to_string())
        .unwrap_or_default()
}
fn set_ul_rate(c: &mut RqbitDesktopConfig, v: SharedString) {
    c.ratelimits.upload_bps = v
        .parse::<u32>()
        .ok()
        .filter(|&v| v > 0)
        .and_then(NonZeroU32::new);
}

fn get_tcp_listen(c: &RqbitDesktopConfig) -> bool {
    c.connections.enable_tcp_listen
}
fn set_tcp_listen(c: &mut RqbitDesktopConfig, v: bool) {
    c.connections.enable_tcp_listen = v;
}

fn get_utp(c: &RqbitDesktopConfig) -> bool {
    c.connections.enable_utp
}
fn set_utp(c: &mut RqbitDesktopConfig, v: bool) {
    c.connections.enable_utp = v;
}

fn get_upnp_fwd(c: &RqbitDesktopConfig) -> bool {
    c.connections.enable_upnp_port_forward
}
fn set_upnp_fwd(c: &mut RqbitDesktopConfig, v: bool) {
    c.connections.enable_upnp_port_forward = v;
}

fn get_listen_port(c: &RqbitDesktopConfig) -> String {
    c.connections.listen_port.to_string()
}
fn set_listen_port(c: &mut RqbitDesktopConfig, v: SharedString) {
    if let Ok(port) = v.parse::<u16>() {
        c.connections.listen_port = port;
    }
}

fn get_tcp_outgoing(c: &RqbitDesktopConfig) -> bool {
    c.connections.enable_tcp_outgoing
}
fn set_tcp_outgoing(c: &mut RqbitDesktopConfig, v: bool) {
    c.connections.enable_tcp_outgoing = v;
}

fn get_socks(c: &RqbitDesktopConfig) -> String {
    c.connections.socks_proxy.clone()
}
fn set_socks(c: &mut RqbitDesktopConfig, v: SharedString) {
    c.connections.socks_proxy = v.to_string();
}

fn get_peer_ct(c: &RqbitDesktopConfig) -> String {
    c.connections.peer_connect_timeout.as_secs().to_string()
}
fn set_peer_ct(c: &mut RqbitDesktopConfig, v: SharedString) {
    if let Ok(secs) = v.parse::<u64>() {
        c.connections.peer_connect_timeout = std::time::Duration::from_secs(secs);
    }
}

fn get_peer_rw(c: &RqbitDesktopConfig) -> String {
    c.connections.peer_read_write_timeout.as_secs().to_string()
}
fn set_peer_rw(c: &mut RqbitDesktopConfig, v: SharedString) {
    if let Ok(secs) = v.parse::<u64>() {
        c.connections.peer_read_write_timeout = std::time::Duration::from_secs(secs);
    }
}

fn get_http_disable(c: &RqbitDesktopConfig) -> bool {
    !c.http_api.disable
}
fn set_http_disable(c: &mut RqbitDesktopConfig, v: bool) {
    c.http_api.disable = !v;
}

fn get_http_ro(c: &RqbitDesktopConfig) -> bool {
    c.http_api.read_only
}
fn set_http_ro(c: &mut RqbitDesktopConfig, v: bool) {
    c.http_api.read_only = v;
}

fn get_http_addr(c: &RqbitDesktopConfig) -> String {
    c.http_api.listen_addr.to_string()
}
fn set_http_addr(c: &mut RqbitDesktopConfig, v: SharedString) {
    if let Ok(addr) = v.parse() {
        c.http_api.listen_addr = addr;
    }
}

fn get_upnp_server(c: &RqbitDesktopConfig) -> bool {
    c.upnp.enable_server
}
fn set_upnp_server(c: &mut RqbitDesktopConfig, v: bool) {
    c.upnp.enable_server = v;
}

fn get_upnp_name(c: &RqbitDesktopConfig) -> String {
    c.upnp.server_friendly_name.clone().unwrap_or_default()
}
fn set_upnp_name(c: &mut RqbitDesktopConfig, v: SharedString) {
    let s = v.to_string();
    c.upnp.server_friendly_name = if s.is_empty() { None } else { Some(s) };
}

// ── Render ─────────────────────────────────────────────────────────────────

impl Render for SettingsPage {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let config = self.config.clone();

        v_flex()
            .size_full()
            .gap_0()
            // Toolbar
            .child(
                h_flex()
                    .gap_2()
                    .pl(px(78.))
                    .pr_2()
                    .py_2()
                    .bg(theme.background)
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        Button::new("settings-back")
                            .label("← Back")
                            .on_click(cx.listener(|this, _, _, cx| this.on_back(cx))),
                    )
                    .child(
                        Button::new("settings-reset")
                            .label("Reset to Default")
                            .on_click(cx.listener(|this, _, _, cx| this.on_reset(cx))),
                    )
                    .child(
                        h_flex().flex_1().justify_end().child(
                            Button::new("settings-apply")
                                .primary()
                                .label("Apply")
                                .on_click(cx.listener(|this, _, _, cx| this.on_apply(cx))),
                        ),
                    ),
            )
            // Settings content
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scrollbar()
                    .child(
                        Settings::new("rqbit-settings")
                            .with_size(Size::Medium)
                            .pages(vec![
                                // ── General ──
                                SettingPage::new("General")
                                    .default_open(true)
                                    .group(SettingGroup::new().title("Download").items(vec![
                                        SettingItem::new(
                                            "Default download folder",
                                            SettingField::render(download_location_field(config.clone())),
                                        )
                                        .layout(Axis::Vertical),
                                    ])),
                                // ── DHT ──
                                SettingPage::new("DHT").group(
                                    SettingGroup::new().title("DHT Settings").items(vec![
                                        SettingItem::new(
                                            "Enable DHT",
                                            switch_field(config.clone(), get_dht_disable, set_dht_disable),
                                        )
                                        .description("Enable the Distributed Hash Table for peer discovery."),
                                        SettingItem::new(
                                            "Enable DHT persistence",
                                            switch_field(config.clone(), get_dht_persist, set_dht_persist),
                                        )
                                        .description("Persist DHT routing table across restarts."),
                                        SettingItem::new(
                                            "DHT persistence filename",
                                            input_field(config.clone(), get_dht_filename, set_dht_filename),
                                        )
                                        .layout(Axis::Vertical),
                                    ]),
                                ),
                                // ── Session ──
                                SettingPage::new("Session")
                                    .group(
                                        SettingGroup::new().title("Persistence").items(vec![
                                            SettingItem::new(
                                                "Enable session persistence",
                                                switch_field(config.clone(), get_persist_disable, set_persist_disable),
                                            )
                                            .description("Save torrent state to disk for resume on restart."),
                                            SettingItem::new(
                                                "Persistence folder",
                                                input_field(config.clone(), get_persist_folder, set_persist_folder),
                                            )
                                            .layout(Axis::Vertical),
                                            SettingItem::new(
                                                "Enable fast resume (experimental)",
                                                switch_field(config.clone(), get_fastresume, set_fastresume),
                                            ),
                                        ]),
                                    )
                                    .group(
                                        SettingGroup::new().title("Rate Limits").items(vec![
                                            SettingItem::new(
                                                "Download rate limit (bytes/sec, 0 = unlimited)",
                                                input_field(config.clone(), get_dl_rate, set_dl_rate),
                                            )
                                            .layout(Axis::Vertical),
                                            SettingItem::new(
                                                "Upload rate limit (bytes/sec, 0 = unlimited)",
                                                input_field(config.clone(), get_ul_rate, set_ul_rate),
                                            )
                                            .layout(Axis::Vertical),
                                        ]),
                                    ),
                                // ── Connection ──
                                SettingPage::new("Connection")
                                    .group(
                                        SettingGroup::new().title("Listening").items(vec![
                                            SettingItem::new(
                                                "Listen on TCP",
                                                switch_field(config.clone(), get_tcp_listen, set_tcp_listen),
                                            ),
                                            SettingItem::new(
                                                "Listen on uTP (over UDP)",
                                                switch_field(config.clone(), get_utp, set_utp),
                                            ),
                                            SettingItem::new(
                                                "Advertise port over UPnP",
                                                switch_field(config.clone(), get_upnp_fwd, set_upnp_fwd),
                                            ),
                                            SettingItem::new(
                                                "Listen port",
                                                input_field(config.clone(), get_listen_port, set_listen_port),
                                            )
                                            .layout(Axis::Vertical),
                                        ]),
                                    )
                                    .group(
                                        SettingGroup::new().title("Outgoing").items(vec![
                                            SettingItem::new(
                                                "Enable outgoing TCP",
                                                switch_field(config.clone(), get_tcp_outgoing, set_tcp_outgoing),
                                            ),
                                            SettingItem::new(
                                                "SOCKS proxy",
                                                input_field(config.clone(), get_socks, set_socks),
                                            )
                                            .layout(Axis::Vertical),
                                            SettingItem::new(
                                                "Peer connect timeout (seconds)",
                                                input_field(config.clone(), get_peer_ct, set_peer_ct),
                                            )
                                            .layout(Axis::Vertical),
                                            SettingItem::new(
                                                "Peer read/write timeout (seconds)",
                                                input_field(config.clone(), get_peer_rw, set_peer_rw),
                                            )
                                            .layout(Axis::Vertical),
                                        ]),
                                    ),
                                // ── HTTP API ──
                                SettingPage::new("HTTP API").group(
                                    SettingGroup::new().title("HTTP API Settings").items(vec![
                                        SettingItem::new(
                                            "Enable HTTP API",
                                            switch_field(config.clone(), get_http_disable, set_http_disable),
                                        ),
                                        SettingItem::new(
                                            "Read only",
                                            switch_field(config.clone(), get_http_ro, set_http_ro),
                                        )
                                        .description(
                                            "When enabled, the HTTP API will not accept state-changing requests.",
                                        ),
                                        SettingItem::new(
                                            "HTTP API listen address",
                                            input_field(config.clone(), get_http_addr, set_http_addr),
                                        )
                                        .layout(Axis::Vertical),
                                    ]),
                                ),
                                // ── UPnP Server ──
                                SettingPage::new("UPnP Server").group(
                                    SettingGroup::new().title("UPnP Media Server").items(vec![
                                        SettingItem::new(
                                            "Enable UPnP media server",
                                            switch_field(config.clone(), get_upnp_server, set_upnp_server),
                                        )
                                        .description(
                                            "Serve downloaded media over UPnP on the local network.",
                                        ),
                                        SettingItem::new(
                                            "UPnP friendly name",
                                            input_field(config, get_upnp_name, set_upnp_name),
                                        )
                                        .layout(Axis::Vertical),
                                    ]),
                                ),
                            ]),
                    ),
            )
    }
}

impl Focusable for SettingsPage {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

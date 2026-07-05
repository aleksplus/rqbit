use gpui::*;
use gpui_component::{
    ActiveTheme as _,
    button::{Button, ButtonVariants},
    form::field,
    group_box::GroupBox,
    h_flex,
    input::{Input, InputState},
    scroll::ScrollableElement,
    switch::Switch,
    v_flex,
};
use std::{num::NonZeroU32, sync::Arc};

use crate::{
    config::{RqbitDesktopConfig, write_config},
    state::State,
};

/// Events emitted by [`ConfigModal`] to communicate with the parent view.
#[derive(Clone, Debug)]
pub enum ConfigModalEvent {
    /// User clicked **Apply** – config was saved, close the modal.
    Applied,
    /// User clicked **Cancel** – close without saving.
    Cancelled,
}

/// Configuration modal for editing rqbit settings.
///
/// Organised into sections: Home, DHT, Session, Connection, HTTP API, UPnP Server.
pub struct ConfigModal {
    state: Arc<State>,
    config: RqbitDesktopConfig,
    original_config: RqbitDesktopConfig,

    // Text input states
    download_dir_input: Entity<InputState>,
    dht_persistence_filename_input: Entity<InputState>,
    persistence_folder_input: Entity<InputState>,
    socks_proxy_input: Entity<InputState>,
    listen_port_input: Entity<InputState>,
    peer_connect_timeout_input: Entity<InputState>,
    peer_read_write_timeout_input: Entity<InputState>,
    http_api_listen_addr_input: Entity<InputState>,
    upnp_friendly_name_input: Entity<InputState>,
    ratelimit_download_input: Entity<InputState>,
    ratelimit_upload_input: Entity<InputState>,

    focus_handle: FocusHandle,
}

impl EventEmitter<ConfigModalEvent> for ConfigModal {}

impl ConfigModal {
    pub fn new(window: &mut Window, cx: &mut Context<Self>, state: Arc<State>) -> Self {
        let config = state.shared().read().config();
        let original_config = config.clone();
        let focus_handle = cx.focus_handle();

        let mut this = Self {
            state,
            config: config.clone(),
            original_config,
            // Placeholder entities – will be replaced by init_inputs below.
            download_dir_input: cx.new(|cx| InputState::new(window, cx)),
            dht_persistence_filename_input: cx.new(|cx| InputState::new(window, cx)),
            persistence_folder_input: cx.new(|cx| InputState::new(window, cx)),
            socks_proxy_input: cx.new(|cx| InputState::new(window, cx)),
            listen_port_input: cx.new(|cx| InputState::new(window, cx)),
            peer_connect_timeout_input: cx.new(|cx| InputState::new(window, cx)),
            peer_read_write_timeout_input: cx.new(|cx| InputState::new(window, cx)),
            http_api_listen_addr_input: cx.new(|cx| InputState::new(window, cx)),
            upnp_friendly_name_input: cx.new(|cx| InputState::new(window, cx)),
            ratelimit_download_input: cx.new(|cx| InputState::new(window, cx)),
            ratelimit_upload_input: cx.new(|cx| InputState::new(window, cx)),
            focus_handle,
        };
        this.init_inputs(window, cx, &config);
        this
    }

    /// (Re-)initialise all input states from the given config.
    fn init_inputs(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        config: &RqbitDesktopConfig,
    ) {
        self.download_dir_input = cx.new(|cx| {
            InputState::new(window, cx).default_value(
                config
                    .default_download_location
                    .to_string_lossy()
                    .to_string(),
            )
        });
        self.dht_persistence_filename_input = cx.new(|cx| {
            InputState::new(window, cx).default_value(
                config
                    .dht
                    .persistence_filename
                    .to_string_lossy()
                    .to_string(),
            )
        });
        self.persistence_folder_input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(config.persistence.folder.to_string_lossy().to_string())
        });
        self.socks_proxy_input = cx.new(|cx| {
            InputState::new(window, cx).default_value(config.connections.socks_proxy.clone())
        });
        self.listen_port_input = cx.new(|cx| {
            InputState::new(window, cx).default_value(config.connections.listen_port.to_string())
        });
        self.peer_connect_timeout_input = cx.new(|cx| {
            InputState::new(window, cx).default_value(
                config
                    .connections
                    .peer_connect_timeout
                    .as_secs()
                    .to_string(),
            )
        });
        self.peer_read_write_timeout_input = cx.new(|cx| {
            InputState::new(window, cx).default_value(
                config
                    .connections
                    .peer_read_write_timeout
                    .as_secs()
                    .to_string(),
            )
        });
        self.http_api_listen_addr_input = cx.new(|cx| {
            InputState::new(window, cx).default_value(config.http_api.listen_addr.to_string())
        });
        self.upnp_friendly_name_input = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value(config.upnp.server_friendly_name.clone().unwrap_or_default())
        });
        self.ratelimit_download_input = cx.new(|cx| {
            InputState::new(window, cx).default_value(
                config
                    .ratelimits
                    .download_bps
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
            )
        });
        self.ratelimit_upload_input = cx.new(|cx| {
            InputState::new(window, cx).default_value(
                config
                    .ratelimits
                    .upload_bps
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
            )
        });
    }

    /// Collect values from input states into the config struct.
    fn sync_inputs_to_config(&mut self, cx: &mut Context<Self>) {
        self.config.default_download_location =
            self.download_dir_input.read(cx).value().to_string().into();

        self.config.dht.persistence_filename = self
            .dht_persistence_filename_input
            .read(cx)
            .value()
            .to_string()
            .into();

        self.config.persistence.folder = self
            .persistence_folder_input
            .read(cx)
            .value()
            .to_string()
            .into();

        self.config.connections.socks_proxy = self.socks_proxy_input.read(cx).value().to_string();

        if let Ok(port) = self.listen_port_input.read(cx).value().parse::<u16>() {
            self.config.connections.listen_port = port;
        }

        if let Ok(secs) = self
            .peer_connect_timeout_input
            .read(cx)
            .value()
            .parse::<u64>()
        {
            self.config.connections.peer_connect_timeout = std::time::Duration::from_secs(secs);
        }

        if let Ok(secs) = self
            .peer_read_write_timeout_input
            .read(cx)
            .value()
            .parse::<u64>()
        {
            self.config.connections.peer_read_write_timeout = std::time::Duration::from_secs(secs);
        }

        if let Ok(addr) = self.http_api_listen_addr_input.read(cx).value().parse() {
            self.config.http_api.listen_addr = addr;
        }

        let friendly = self.upnp_friendly_name_input.read(cx).value().to_string();
        self.config.upnp.server_friendly_name = if friendly.is_empty() {
            None
        } else {
            Some(friendly)
        };

        let dl = self.ratelimit_download_input.read(cx).value();
        self.config.ratelimits.download_bps = dl
            .parse::<u32>()
            .ok()
            .filter(|&v| v > 0)
            .and_then(NonZeroU32::new);

        let ul = self.ratelimit_upload_input.read(cx).value();
        self.config.ratelimits.upload_bps = ul
            .parse::<u32>()
            .ok()
            .filter(|&v| v > 0)
            .and_then(NonZeroU32::new);
    }

    fn save_config(&mut self, cx: &mut Context<Self>) {
        self.sync_inputs_to_config(cx);

        // Write config to disk
        if let Err(e) = write_config(&self.state.config_filename, &self.config) {
            eprintln!("Error writing config: {:?}", e);
        }

        // Reconfigure the session with new config
        let state = self.state.clone();
        let config = self.config.clone();
        let entity_id = cx.entity_id();
        cx.spawn(async move |_, cx| {
            if let Err(e) = state.configure(config).await {
                eprintln!("Error reconfiguring session: {:?}", e);
            }
            cx.update(|cx| cx.notify(entity_id));
        })
        .detach();

        cx.emit(ConfigModalEvent::Applied);
    }

    fn reset_to_default(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let default = RqbitDesktopConfig::default();
        self.config = default.clone();
        self.init_inputs(window, cx, &default);
        cx.notify();
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        // Restore original config (no changes persisted)
        self.config = self.original_config.clone();
        cx.emit(ConfigModalEvent::Cancelled);
    }
}

impl Render for ConfigModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        v_flex()
            .size_full()
            .gap_4()
            .child(
                // Scrollable settings content
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .gap_4()
                    .overflow_y_scrollbar()
                    // ── Home ──
                    .child(
                        GroupBox::new().title("Home").child(
                            v_flex().gap_3().child(
                                field()
                                    .label("Default download folder")
                                    .child(Input::new(&self.download_dir_input)),
                            ),
                        ),
                    )
                    // ── DHT ──
                    .child(
                        GroupBox::new().title("DHT").child(
                            v_flex()
                                .gap_3()
                                .child(
                                    field().label("Enable DHT").child(
                                        Switch::new("dht_enable")
                                            .checked(!self.config.dht.disable)
                                            .on_click(cx.listener(
                                                |this, checked: &bool, _, cx| {
                                                    this.config.dht.disable = !*checked;
                                                    cx.notify();
                                                },
                                            )),
                                    ),
                                )
                                .child(
                                    field().label("Enable DHT persistence").child(
                                        Switch::new("dht_persist")
                                            .checked(!self.config.dht.disable_persistence)
                                            .on_click(cx.listener(
                                                |this, checked: &bool, _, cx| {
                                                    this.config.dht.disable_persistence = !*checked;
                                                    cx.notify();
                                                },
                                            )),
                                    ),
                                )
                                .child(
                                    field()
                                        .label("DHT persistence filename")
                                        .child(Input::new(&self.dht_persistence_filename_input)),
                                ),
                        ),
                    )
                    // ── Session ──
                    .child(
                        GroupBox::new().title("Session").child(
                            v_flex()
                                .gap_3()
                                .child(
                                    field().label("Enable session persistence").child(
                                        Switch::new("session_persist")
                                            .checked(!self.config.persistence.disable)
                                            .on_click(cx.listener(
                                                |this, checked: &bool, _, cx| {
                                                    this.config.persistence.disable = !*checked;
                                                    cx.notify();
                                                },
                                            )),
                                    ),
                                )
                                .child(
                                    field()
                                        .label("Persistence folder")
                                        .child(Input::new(&self.persistence_folder_input)),
                                )
                                .child(
                                    field().label("Enable fast resume (experimental)").child(
                                        Switch::new("fastresume")
                                            .checked(self.config.persistence.fastresume)
                                            .on_click(cx.listener(
                                                |this, checked: &bool, _, cx| {
                                                    this.config.persistence.fastresume = *checked;
                                                    cx.notify();
                                                },
                                            )),
                                    ),
                                )
                                .child(
                                    field()
                                        .label("Download rate limit (bytes/sec, 0 = unlimited)")
                                        .child(Input::new(&self.ratelimit_download_input)),
                                )
                                .child(
                                    field()
                                        .label("Upload rate limit (bytes/sec, 0 = unlimited)")
                                        .child(Input::new(&self.ratelimit_upload_input)),
                                ),
                        ),
                    )
                    // ── Connection ──
                    .child(
                        GroupBox::new().title("Connection").child(
                            v_flex()
                                .gap_3()
                                .child(
                                    field().label("Listen on TCP").child(
                                        Switch::new("tcp_listen")
                                            .checked(self.config.connections.enable_tcp_listen)
                                            .on_click(cx.listener(
                                                |this, checked: &bool, _, cx| {
                                                    this.config.connections.enable_tcp_listen =
                                                        *checked;
                                                    cx.notify();
                                                },
                                            )),
                                    ),
                                )
                                .child(
                                    field().label("Listen on uTP (over UDP)").child(
                                        Switch::new("utp_listen")
                                            .checked(self.config.connections.enable_utp)
                                            .on_click(cx.listener(
                                                |this, checked: &bool, _, cx| {
                                                    this.config.connections.enable_utp = *checked;
                                                    cx.notify();
                                                },
                                            )),
                                    ),
                                )
                                .child(
                                    field().label("Advertise port over UPnP").child(
                                        Switch::new("upnp_port_forward")
                                            .checked(
                                                self.config.connections.enable_upnp_port_forward,
                                            )
                                            .on_click(cx.listener(
                                                |this, checked: &bool, _, cx| {
                                                    this.config
                                                        .connections
                                                        .enable_upnp_port_forward = *checked;
                                                    cx.notify();
                                                },
                                            )),
                                    ),
                                )
                                .child(
                                    field().label("Enable outgoing TCP").child(
                                        Switch::new("tcp_outgoing")
                                            .checked(self.config.connections.enable_tcp_outgoing)
                                            .on_click(cx.listener(
                                                |this, checked: &bool, _, cx| {
                                                    this.config.connections.enable_tcp_outgoing =
                                                        *checked;
                                                    cx.notify();
                                                },
                                            )),
                                    ),
                                )
                                .child(
                                    field()
                                        .label("SOCKS proxy")
                                        .child(Input::new(&self.socks_proxy_input)),
                                )
                                .child(
                                    field()
                                        .label("Listen port")
                                        .child(Input::new(&self.listen_port_input)),
                                )
                                .child(
                                    field()
                                        .label("Peer connect timeout (seconds)")
                                        .child(Input::new(&self.peer_connect_timeout_input)),
                                )
                                .child(
                                    field()
                                        .label("Peer read/write timeout (seconds)")
                                        .child(Input::new(&self.peer_read_write_timeout_input)),
                                ),
                        ),
                    )
                    // ── HTTP API ──
                    .child(
                        GroupBox::new().title("HTTP API").child(
                            v_flex()
                                .gap_3()
                                .child(
                                    field().label("Enable HTTP API").child(
                                        Switch::new("http_api_enable")
                                            .checked(!self.config.http_api.disable)
                                            .on_click(cx.listener(
                                                |this, checked: &bool, _, cx| {
                                                    this.config.http_api.disable = !*checked;
                                                    cx.notify();
                                                },
                                            )),
                                    ),
                                )
                                .child(
                                    field().label("Read only").child(
                                        Switch::new("http_api_readonly")
                                            .checked(self.config.http_api.read_only)
                                            .on_click(cx.listener(
                                                |this, checked: &bool, _, cx| {
                                                    this.config.http_api.read_only = *checked;
                                                    cx.notify();
                                                },
                                            )),
                                    ),
                                )
                                .child(
                                    field()
                                        .label("HTTP API listen address")
                                        .child(Input::new(&self.http_api_listen_addr_input)),
                                ),
                        ),
                    )
                    // ── UPnP Server ──
                    .child(
                        GroupBox::new().title("UPnP Server").child(
                            v_flex()
                                .gap_3()
                                .child(
                                    field().label("Enable UPnP media server").child(
                                        Switch::new("upnp_server")
                                            .checked(self.config.upnp.enable_server)
                                            .on_click(cx.listener(
                                                |this, checked: &bool, _, cx| {
                                                    this.config.upnp.enable_server = *checked;
                                                    cx.notify();
                                                },
                                            )),
                                    ),
                                )
                                .child(
                                    field()
                                        .label("UPnP friendly name")
                                        .child(Input::new(&self.upnp_friendly_name_input)),
                                ),
                        ),
                    ),
            )
            // ── Button bar ──
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .pt_2()
                    .border_t_1()
                    .border_color(theme.border)
                    .child(
                        Button::new("reset_to_default")
                            .label("Reset to Default")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.reset_to_default(window, cx);
                            })),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(Button::new("cancel").outline().label("Cancel").on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.cancel(cx);
                                }),
                            ))
                            .child(Button::new("apply").primary().label("Apply").on_click(
                                cx.listener(|this, _, _, cx| {
                                    this.save_config(cx);
                                }),
                            )),
                    ),
            )
    }
}

impl Focusable for ConfigModal {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

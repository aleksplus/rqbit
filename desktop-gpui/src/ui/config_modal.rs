use gpui::*;
use gpui_component::{
    form::{Checkbox, Form, InputField},
    ActiveTheme as _, Button, Modal, StyledExt as _, v_flex,
};
use std::sync::Arc;

use crate::{
    config::{read_config, write_config, RqbitDesktopConfig},
    state::{State, SharedState},
};

/// Configuration modal for editing rqbit settings
pub struct ConfigModal {
    state: Arc<State>,
    config: RqbitDesktopConfig,
    focus_handle: FocusHandle,
}

impl ConfigModal {
    pub fn new(window: &mut Window, cx: &mut Context<Self>, state: Arc<State>) -> Self {
        let config = state.shared.read().config();
        let focus_handle = cx.focus_handle();

        Self {
            state,
            config,
            focus_handle,
        }
    }

    fn save_config(&mut self, cx: &mut Context<Self>) {
        // Write config to disk
        if let Err(e) = write_config(&self.state.config_filename, &self.config) {
            eprintln!("Error writing config: {:?}", e);
        }

        // Reconfigure the session with new config
        let state = self.state.clone();
        let config = self.config.clone();
        cx.spawn(async move |cx| {
            if let Err(e) = state.configure(config).await {
                eprintln!("Error reconfiguring session: {:?}", e);
            }
            // Notify that config changed
            cx.notify();
        })
        .detach();
    }
}

impl Render for ConfigModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .p_4()
            .gap_4()
            .child(
                Form::new()
                    .field(
                        InputField::new("download_dir", "Download Directory")
                            .placeholder("Enter download directory")
                            .value(self.config.default_download_location.to_string_lossy())
                            .on_change(cx.listener(|this, value: &String, cx| {
                                this.config.default_download_location = value.into();
                                cx.notify();
                            })),
                    )
                    .field(
                        Checkbox::new("dht.disable", "Disable DHT")
                            .checked(self.config.dht.disable)
                            .on_change(cx.listener(|this, checked: bool, cx| {
                                this.config.dht.disable = checked;
                                cx.notify();
                            })),
                    )
                    .field(
                        Checkbox::new("dht.disable_persistence", "Disable DHT Persistence")
                            .checked(self.config.dht.disable_persistence)
                            .on_change(cx.listener(|this, checked: bool, cx| {
                                this.config.dht.disable_persistence = checked;
                                cx.notify();
                            })),
                    )
                    .field(
                        Checkbox::new("upnp.enable_server", "Enable UPnP Server")
                            .checked(self.config.upnp.enable_server)
                            .on_change(cx.listener(|this, checked: bool, cx| {
                                this.config.upnp.enable_server = checked;
                                cx.notify();
                            })),
                    )
                    .field(
                        Checkbox::new("connections.enable_utp", "Enable uTP")
                            .checked(self.config.connections.enable_utp)
                            .on_change(cx.listener(|this, checked: bool, cx| {
                                this.config.connections.enable_utp = checked;
                                cx.notify();
                            })),
                    )
                    .field(
                        Checkbox::new("connections.enable_upnp_port_forward", "Enable UPnP Port Forwarding")
                            .checked(self.config.connections.enable_upnp_port_forward)
                            .on_change(cx.listener(|this, checked: bool, cx| {
                                this.config.connections.enable_upnp_port_forward = checked;
                                cx.notify();
                            })),
                    )
                    .field(
                        InputField::new("connections.listen_port", "Listen Port")
                            .placeholder("4240")
                            .value(self.config.connections.listen_port.to_string())
                            .on_change(cx.listener(|this, value: &String, cx| {
                                if let Ok(port) = value.parse::<u16>() {
                                    this.config.connections.listen_port = port;
                                }
                                cx.notify();
                            })),
                    )
                    .field(
                        Checkbox::new("http_api.disable", "Disable HTTP API")
                            .checked(self.config.http_api.disable)
                            .on_change(cx.listener(|this, checked: bool, cx| {
                                this.config.http_api.disable = checked;
                                cx.notify();
                            })),
                    )
                    .field(
                        InputField::new("http_api.listen_addr", "HTTP API Listen Address")
                            .placeholder("127.0.0.1:3030")
                            .value(self.config.http_api.listen_addr.to_string())
                            .on_change(cx.listener(|this, value: &String, cx| {
                                if let Ok(addr) = value.parse() {
                                    this.config.http_api.listen_addr = addr;
                                }
                                cx.notify();
                            })),
                    )
                    .field(
                        Checkbox::new("persistence.disable", "Disable Persistence")
                            .checked(self.config.persistence.disable)
                            .on_change(cx.listener(|this, checked: bool, cx| {
                                this.config.persistence.disable = checked;
                                cx.notify();
                            })),
                    )
                    .field(
                        Checkbox::new("persistence.fastresume", "Enable Fast Resume")
                            .checked(self.config.persistence.fastresume)
                            .on_change(cx.listener(|this, checked: bool, cx| {
                                this.config.persistence.fastresume = checked;
                                cx.notify();
                            })),
                    ),
            )
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("cancel")
                            .label("Cancel")
                            .on_click(cx.listener(|this, _, cx| {
                                cx.dismiss();
                            })),
                    )
                    .child(
                        Button::new("save")
                            .label("Save")
                            .primary()
                            .on_click(cx.listener(|this, _, cx| {
                                this.save_config(cx);
                                cx.dismiss();
                            })),
                    ),
            )
    }
}

impl Focusable for ConfigModal {
    fn focus_handle(&self) -> FocusHandle {
        self.focus_handle.clone()
    }
}
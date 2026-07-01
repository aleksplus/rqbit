use gpui::*;
use gpui_component::{
    button::Button,
    form::{Form, FormField},
    ActiveTheme as _, Modal, StyledExt as _, v_flex,
};
use std::sync::Arc;

use crate::{
    config::{RqbitDesktopConfig, write_config},
    state::{State},
};

/// Configuration modal for editing rqbit settings
pub struct ConfigModal {
    state: Arc<State>,
    config: RqbitDesktopConfig,
    focus_handle: FocusHandle,
}

impl ConfigModal {
    pub fn new(window: &mut Window, cx: &mut Context<Self>, state: Arc<State>) -> Self {
        let config = state.shared().read().config();
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
        cx.spawn(async move |_, cx| {
            if let Err(e) = state.configure(config).await {
                eprintln!("Error reconfiguring session: {:?}", e);
            }
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
                    .field(FormField::new("download_dir")
                        .label("Download Directory")
                        .input(
                            gpui_component::input::TextInput::new(&mut self.config.default_download_location.to_string_lossy().to_string())
                                .on_change(cx.listener(|this, value: String, cx| {
                                    this.config.default_download_location = value.into();
                                    cx.notify();
                                }))
                        ))
                    .field(FormField::new("dht_disable")
                        .label("Disable DHT")
                        .input(
                            gpui_component::input::Checkbox::new()
                                .checked(self.config.dht.disable)
                                .on_change(cx.listener(|this, checked: bool, cx| {
                                    this.config.dht.disable = checked;
                                    cx.notify();
                                }))
                        ))
                    .field(FormField::new("upnp_enable")
                        .label("Enable UPnP Server")
                        .input(
                            gpui_component::input::Checkbox::new()
                                .checked(self.config.upnp.enable_server)
                                .on_change(cx.listener(|this, checked: bool, cx| {
                                    this.config.upnp.enable_server = checked;
                                    cx.notify();
                                }))
                        ))
                    .field(FormField::new("listen_port")
                        .label("Listen Port")
                        .input(
                            gpui_component::input::TextInput::new(&mut self.config.connections.listen_port.to_string())
                                .on_change(cx.listener(|this, value: String, cx| {
                                    if let Ok(port) = value.parse::<u16>() {
                                        this.config.connections.listen_port = port;
                                    }
                                    cx.notify();
                                }))
                        )),
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
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
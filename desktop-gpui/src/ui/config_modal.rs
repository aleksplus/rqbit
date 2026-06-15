use gpui::{Context, Div, IntoElement, Render, SharedString, div, prelude::*};
use gpui_component::{Button, Checkbox, Form, InputField, Modal};
use librqbit::config::RqbitDesktopConfig;
use std::sync::{Arc, Mutex};

/// Props for the configuration modal.
pub struct ConfigModalProps {
    pub show: bool,
    /// Callback invoked when the user confirms changes.
    pub on_save: Arc<dyn Fn(RqbitDesktopConfig) + Send + Sync>,
    /// Current configuration.
    pub config: RqbitDesktopConfig,
}

/// The modal component.
pub struct ConfigModal {
    props: ConfigModalProps,
    local_config: Mutex<RqbitDesktopConfig>,
}

impl ConfigModal {
    pub fn new(props: ConfigModalProps) -> Self {
        Self {
            props,
            local_config: Mutex::new(props.config.clone()),
        }
    }

    fn update_field<F>(&self, cx: &mut Context<Self>, field_name: &'static str, f: F)
    where
        F: FnOnce(&mut RqbitDesktopConfig),
    {
        let mut cfg = self.local_config.lock().unwrap();
        f(&mut cfg);
    }
}

impl Render for ConfigModal {
    fn render(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.props.show {
            return div();
        }

        let on_save = self.props.on_save.clone();
        let local_cfg_clone = { self.local_config.lock().unwrap().clone() };

        // Helper closures for form fields.
        let on_download_folder =
            cx.new_event_handler(move |cx, e: gpui::Event<gpui::text_input::TextChanged>| {
                let text = e.text();
                self.update_field(cx, "default_download_location", |cfg| {
                    cfg.default_download_location = std::path::PathBuf::from(text)
                });
            });

        let on_disable_upload =
            cx.new_event_handler(move |cx, _: gpui::Event<gpui::checkbox::Changed>| {
                self.update_field(cx, "disable_upload", |cfg| {
                    cfg.disable_upload = !cfg.disable_upload
                });
            });

        // In a full implementation we would add handlers for all fields.

        let body = Form::new()
            .field(
                InputField::text("Default download folder", "default_download_location")
                    .value(&local_cfg_clone.default_download_location.to_string_lossy())
                    .on_change(on_download_folder),
            )
            // Placeholder for disable upload; only compiled if feature present
            .field(
                Checkbox::new("Disable upload", "disable_upload")
                    .checked(local_cfg_clone.disable_upload)
                    .on_change(on_disable_upload),
            );

        Modal::new()
            .title("Configure Rqbit desktop")
            .body(body)
            .footer(Button::new("Save").on_click(move |_, cx| {
                let cfg = self.local_config.lock().unwrap().clone();
                (on_save)(cfg);
            }))
    }
}

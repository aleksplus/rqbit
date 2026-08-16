use gpui::{FocusHandle, *};
use gpui_component::{
    ActiveTheme as _,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputState},
    v_flex,
};

/// Events emitted by [`DeleteDialog`].
#[derive(Clone, Debug)]
pub enum DeleteDialogEvent {
    /// User cancelled.
    Cancelled,
    /// User confirmed deletion with (ids, delete_files).
    Confirmed(Vec<usize>, bool),
}

/// A modal dialog for confirming torrent deletion.
pub struct DeleteDialog {
    ids: Vec<usize>,
    names: Vec<String>,
    is_bulk: bool,
    delete_files: bool,
    focus_handle: FocusHandle,
}

impl EventEmitter<DeleteDialogEvent> for DeleteDialog {}

impl DeleteDialog {
    pub fn new(
        ids: Vec<usize>,
        names: Vec<String>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        Self {
            is_bulk: ids.len() > 1,
            ids,
            names,
            delete_files: true,
            focus_handle: cx.focus_handle(),
        }
    }

    fn on_cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(DeleteDialogEvent::Cancelled);
    }

    fn on_confirm(&mut self, cx: &mut Context<Self>) {
        cx.emit(DeleteDialogEvent::Confirmed(
            self.ids.clone(),
            self.delete_files,
        ));
    }
}

impl Render for DeleteDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        v_flex()
            .gap_3()
            .child(
                div()
                    .text_size(px(16.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(if self.is_bulk {
                        format!("Delete {} torrents", self.ids.len())
                    } else {
                        "Delete torrent".to_string()
                    }),
            )
            .child(div().child(if self.is_bulk {
                "Are you sure you want to delete the following torrents?"
            } else {
                "Are you sure you want to delete this torrent?"
            }))
            .child(
                div()
                    .rounded_md()
                    .bg(theme.muted)
                    .p_3()
                    .max_h(px(200.))
                    .overflow_y_hidden()
                    .children(
                        self.names
                            .iter()
                            .map(|name| div().text_color(theme.foreground).child(name.clone())),
                    ),
            )
            .child(
                Checkbox::new("delete-files")
                    .checked(self.delete_files)
                    .label("Also delete downloaded files")
                    .on_click(
                        cx.listener(|this: &mut DeleteDialog, checked: &bool, _, _| {
                            this.delete_files = *checked;
                        }),
                    ),
            )
            .child(
                h_flex()
                    .gap_2()
                    .justify_end()
                    .child(
                        Button::new("del-cancel")
                            .outline()
                            .label("Cancel")
                            .on_click(cx.listener(|this, _, _, cx| this.on_cancel(cx))),
                    )
                    .child(
                        Button::new("del-ok")
                            .danger()
                            .label(if self.is_bulk {
                                format!("Delete {} Torrents", self.ids.len())
                            } else {
                                "Delete Torrent".to_string()
                            })
                            .on_click(cx.listener(|this, _, _, cx| this.on_confirm(cx))),
                    ),
            )
    }
}

impl Focusable for DeleteDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// Events emitted by [`MagnetDialog`].
#[derive(Clone, Debug)]
pub enum MagnetDialogEvent {
    /// User clicked **Cancel**.
    Cancelled,
    /// User clicked **Add** and provided a magnet URL.
    Submitted(String),
}

/// A small dialog for entering a magnet link URL.
pub struct MagnetDialog {
    input_state: Entity<InputState>,
    focus_handle: FocusHandle,
}

impl EventEmitter<MagnetDialogEvent> for MagnetDialog {}

impl MagnetDialog {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            input_state: cx
                .new(|cx| InputState::new(window, cx).placeholder("magnet:?xt=urn:btih:...")),
            focus_handle: cx.focus_handle(),
        }
    }

    fn on_cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(MagnetDialogEvent::Cancelled);
    }

    fn on_submit(&mut self, cx: &mut Context<Self>) {
        let value = self.input_state.read(cx).value().to_string();
        if !value.trim().is_empty() {
            cx.emit(MagnetDialogEvent::Submitted(value.trim().to_string()));
        }
    }
}

impl Render for MagnetDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        v_flex()
            .gap_3()
            .child(
                div()
                    .text_size(px(16.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("Add Magnet Link"),
            )
            .child(Input::new(&self.input_state))
            .child(
                h_flex()
                    .gap_2()
                    .justify_end()
                    .child(
                        Button::new("magnet-cancel")
                            .outline()
                            .label("Cancel")
                            .on_click(cx.listener(|this, _, _, cx| this.on_cancel(cx))),
                    )
                    .child(
                        Button::new("magnet-add")
                            .primary()
                            .label("Add")
                            .on_click(cx.listener(|this, _, _, cx| this.on_submit(cx))),
                    ),
            )
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(theme.muted_foreground)
                    .child("Tip: paste a magnet link or a 40-char info hash."),
            )
    }
}

impl Focusable for MagnetDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

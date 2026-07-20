use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_component::{
    ActiveTheme as _,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    h_flex,
    table::{DataTable, TableState},
    v_flex,
};

use crate::ui::file_table::{FileRow, FileTableDelegate};

/// The original source of a torrent, retained so it can be re-added for real
/// after its file list has been resolved via `list_only`.
#[derive(Clone, Debug)]
pub enum AddTorrentSource {
    Bytes(Vec<u8>),
    Url(String),
}

impl AddTorrentSource {
    /// Build a fresh [`librqbit::AddTorrent`] from this source.
    pub fn to_add(&self) -> librqbit::AddTorrent<'static> {
        match self {
            AddTorrentSource::Bytes(b) => librqbit::AddTorrent::from_bytes(b.clone()),
            AddTorrentSource::Url(s) => librqbit::AddTorrent::from_url(s.clone()),
        }
    }
}

/// Events emitted by [`AddTorrentDialog`].
#[derive(Clone, Debug)]
pub enum AddTorrentDialogEvent {
    /// User cancelled the dialog.
    Cancelled,
    /// A single torrent should be added with the given selected file indices.
    /// `None` means "all files" (no selection restriction). The bool is whether
    /// to overwrite existing files on disk.
    AddOne(AddTorrentSource, Option<Vec<usize>>, bool),
    /// All torrents have been processed; the dialog can close.
    Finished,
}

/// A single torrent queued for addition, with its (already resolved) file list.
pub struct AddTorrentEntry {
    pub source: AddTorrentSource,
    pub name: String,
    /// Resolved file rows. Empty means "add all files" (no selection UI).
    pub files: Vec<FileRow>,
}

/// A modal dialog that lets the user pick which files of one or more torrents
/// to download before actually adding them.
pub struct AddTorrentDialog {
    entries: Vec<AddTorrentEntry>,
    current: usize,
    table_state: Entity<TableState<FileTableDelegate>>,
    /// Whether to overwrite existing files (required to resume/seed into them).
    overwrite: bool,
    focus_handle: FocusHandle,
}

impl EventEmitter<AddTorrentDialogEvent> for AddTorrentDialog {}

impl AddTorrentDialog {
    pub fn new(entries: Vec<AddTorrentEntry>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let table_state = cx.new(|cx| {
            let mut delegate = FileTableDelegate::new();
            delegate.set_table(cx.entity().downgrade());
            TableState::new(delegate, window, cx).row_selectable(false)
        });

        let mut this = Self {
            entries,
            current: 0,
            table_state,
            overwrite: false,
            focus_handle: cx.focus_handle(),
        };
        this.load_current(cx);
        this
    }

    /// Whether any of the current torrent's files already exist on disk.
    fn current_has_existing(&self) -> bool {
        self.entries
            .get(self.current)
            .map(|e| e.files.iter().any(|f| f.exists))
            .unwrap_or(false)
    }

    /// Load the file rows for the current torrent into the table.
    fn load_current(&mut self, cx: &mut Context<Self>) {
        let files: Vec<FileRow> = self
            .entries
            .get(self.current)
            .map(|e| e.files.clone())
            .unwrap_or_default();
        let _ = self.table_state.update(cx, |state, cx| {
            state.delegate_mut().rows = files;
            cx.notify();
        });
        cx.notify();
    }

    fn current_name(&self) -> String {
        self.entries
            .get(self.current)
            .map(|e| e.name.clone())
            .unwrap_or_default()
    }

    fn has_more(&self) -> bool {
        self.current + 1 < self.entries.len()
    }

    fn on_cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(AddTorrentDialogEvent::Cancelled);
    }

    /// Add the current torrent with the selected files and advance.
    fn on_add_current(&mut self, cx: &mut Context<Self>) {
        if self.current >= self.entries.len() {
            cx.emit(AddTorrentDialogEvent::Finished);
            return;
        }

        // Take ownership of the current entry (AddTorrentSource is Clone, but we
        // remove it so the next entry shifts into `self.current`).
        let entry = self.entries.remove(self.current);
        let selection = if entry.files.is_empty() {
            None
        } else {
            Some(self.table_state.read(cx).delegate().selected_indices())
        };
        cx.emit(AddTorrentDialogEvent::AddOne(
            entry.source,
            selection,
            self.overwrite,
        ));

        if self.entries.is_empty() {
            cx.emit(AddTorrentDialogEvent::Finished);
        } else {
            // `remove` shifted the next entry into `self.current`.
            self.load_current(cx);
        }
    }

    fn on_select_all(&mut self, included: bool, cx: &mut Context<Self>) {
        let _ = self.table_state.update(cx, |state, cx| {
            for r in state.delegate_mut().rows.iter_mut() {
                r.included = included;
            }
            cx.notify();
        });
    }

    /// Toggle whether existing files should be overwritten when adding.
    fn on_toggle_overwrite(&mut self, cx: &mut Context<Self>) {
        self.overwrite = !self.overwrite;
        cx.notify();
    }
}

impl Render for AddTorrentDialog {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let name = self.current_name();
        let total = self.entries.len();
        let has_files = self
            .entries
            .get(self.current)
            .map(|e| !e.files.is_empty())
            .unwrap_or(false);
        let has_existing = self.current_has_existing();

        v_flex()
            .gap_3()
            .child(
                div()
                    .text_size(px(16.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(if total > 1 {
                        format!("Add Torrent ({}/{})", self.current + 1, total)
                    } else {
                        "Add Torrent".to_string()
                    }),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .text_color(theme.muted_foreground)
                    .child(name),
            )
            .when(has_existing, |this| {
                this.child(
                    div()
                        .p_2()
                        .rounded_md()
                        .border_1()
                        .border_color(gpui::rgb(0xb45309))
                        .bg(gpui::rgb(0x422006))
                        .text_color(gpui::rgb(0xfbbf24))
                        .text_size(px(12.))
                        .child(
                            "Some files already exist at the destination. They will be kept \
                             and only missing pieces downloaded. Enable \"Overwrite existing \
                             files\" below to replace them instead (also required to resume/seed).",
                        ),
                )
            })
            .child(if has_files {
                div()
                    .flex_1()
                    .min_h(px(200.))
                    .border_1()
                    .border_color(theme.border)
                    .rounded_md()
                    .overflow_hidden()
                    .child(DataTable::new(&self.table_state))
                    .into_any_element()
            } else {
                div()
                    .text_color(gpui::rgb(0x888888))
                    .child("No file selection available – all files will be downloaded.")
                    .into_any_element()
            })
            .child(
                h_flex()
                    .gap_2()
                    .when(has_files, |this| {
                        this.child(
                            Button::new("sel-all")
                                .outline()
                                .label("Select All Files")
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.on_select_all(true, cx)),
                                ),
                        )
                        .child(
                            Button::new("sel-none")
                                .outline()
                                .label("Deselect All")
                                .on_click(
                                    cx.listener(|this, _, _, cx| this.on_select_all(false, cx)),
                                ),
                        )
                    })
                    .justify_between()
                    .child(
                        Checkbox::new("overwrite-existing")
                            .checked(self.overwrite)
                            .label("Overwrite existing files")
                            .on_click(
                                cx.listener(|this, _checked, _, cx| this.on_toggle_overwrite(cx)),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("add-cancel")
                                    .outline()
                                    .label("Cancel")
                                    .on_click(cx.listener(|this, _, _, cx| this.on_cancel(cx))),
                            )
                            .child(
                                Button::new("add-ok")
                                    .primary()
                                    .label(if self.has_more() {
                                        "Add & Next"
                                    } else {
                                        "Add Torrent"
                                    })
                                    .on_click(
                                        cx.listener(|this, _, _, cx| this.on_add_current(cx)),
                                    ),
                            ),
                    ),
            )
    }
}

impl Focusable for AddTorrentDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

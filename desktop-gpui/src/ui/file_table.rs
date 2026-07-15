use gpui::*;
use gpui_component::{
    checkbox::Checkbox,
    table::{Column, TableDelegate, TableState},
};
use std::sync::Arc;

/// A single row in a file-selection table.
#[derive(Clone)]
pub struct FileRow {
    pub file_index: usize,
    pub name: String,
    pub length: u64,
    pub included: bool,
}

/// Reusable table delegate for selecting which files of a torrent to download.
///
/// It is self-contained: toggling a checkbox mutates its own rows. An optional
/// `on_toggle` / `on_toggle_all` callback can be supplied for side-effects
/// (e.g. persisting the selection to the API). The owning [`TableState`] must be
/// registered via [`FileTableDelegate::set_table`] so the checkboxes can mutate
/// the rows.
pub struct FileTableDelegate {
    pub rows: Vec<FileRow>,
    columns: Vec<Column>,
    table: WeakEntity<TableState<FileTableDelegate>>,
    pub on_toggle: Option<Arc<dyn Fn(usize, bool, &mut App) + Send + Sync>>,
    pub on_toggle_all: Option<Arc<dyn Fn(bool, &mut App) + Send + Sync>>,
}

impl FileTableDelegate {
    pub fn new() -> Self {
        Self {
            rows: Vec::new(),
            table: WeakEntity::new_invalid(),
            on_toggle: None,
            on_toggle_all: None,
            columns: vec![
                Column::new("included", "Included")
                    .width(35.)
                    .text_center()
                    .selectable(false),
                Column::new("name", "File Name").width(300.),
                Column::new("size", "Size").width(100.),
            ],
        }
    }

    /// Register the owning [`TableState`] so checkboxes can mutate the rows.
    pub fn set_table(&mut self, table: WeakEntity<TableState<FileTableDelegate>>) {
        self.table = table;
    }

    /// Currently selected file indices (those with `included == true`).
    pub fn selected_indices(&self) -> Vec<usize> {
        self.rows
            .iter()
            .filter(|r| r.included)
            .map(|r| r.file_index)
            .collect()
    }
}

impl TableDelegate for FileTableDelegate {
    fn columns_count(&self, _: &App) -> usize {
        self.columns.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        self.columns[col_ix].clone()
    }

    /// Render a "select all" checkbox in the header of the Included column.
    fn render_th(
        &mut self,
        col_ix: usize,
        _: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let col = &self.columns[col_ix];
        if col.key.as_ref() != "included" {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(col.name.clone())
                .into_any_element();
        }

        let all_included = !self.rows.is_empty() && self.rows.iter().all(|r| r.included);
        let new_state = !all_included;
        let table = self.table.clone();
        let on_toggle_all = self.on_toggle_all.clone();

        Checkbox::new("file-included-all")
            .checked(all_included)
            .on_click(move |_checked, _window, cx| {
                if let Some(state) = table.upgrade() {
                    state.update(cx, |s, cx| {
                        for r in s.delegate_mut().rows.iter_mut() {
                            r.included = new_state;
                        }
                        cx.notify();
                    });
                }
                if let Some(cb) = &on_toggle_all {
                    cb(new_state, cx);
                }
            })
            .into_any_element()
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let row = &self.rows[row_ix];
        let col = &self.columns[col_ix];

        match col.key.as_ref() {
            "name" => div().child(row.name.clone()).into_any_element(),
            "size" => div().child(format_bytes(row.length)).into_any_element(),
            "included" => {
                let new_included = !row.included;
                let table = self.table.clone();
                let on_toggle = self.on_toggle.clone();
                Checkbox::new(("file-included", row_ix))
                    .checked(row.included)
                    .on_click(move |_checked, _window, cx| {
                        if let Some(state) = table.upgrade() {
                            state.update(cx, |s, cx| {
                                if let Some(r) = s.delegate_mut().rows.get_mut(row_ix) {
                                    r.included = new_included;
                                }
                                cx.notify();
                            });
                        }
                        if let Some(cb) = &on_toggle {
                            cb(row_ix, new_included, cx);
                        }
                    })
                    .into_any_element()
            }
            _ => div().into_any_element(),
        }
    }
}

/// Format a byte count in human-readable form (binary units).
pub fn format_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;

    let b = bytes as f64;
    if b >= TB {
        format!("{:.2} TiB", b / TB)
    } else if b >= GB {
        format!("{:.2} GiB", b / GB)
    } else if b >= MB {
        format!("{:.2} MiB", b / MB)
    } else if b >= KB {
        format!("{:.2} KiB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}

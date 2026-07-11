use gpui::*;
use gpui_component::{
    ActiveTheme as _,
    button::Button,
    h_flex,
    scroll::ScrollableElement,
    table::{Column, DataTable, TableDelegate, TableState},
    v_flex,
};
use librqbit::api::TorrentDetailsResponse;
use std::sync::Arc;

use crate::state::State;

/// Events emitted by [`TorrentDetailPanel`] to communicate with the parent view.
#[derive(Clone, Debug)]
pub enum TorrentDetailPanelEvent {
    /// User clicked **Back** – close the detail panel.
    Back,
}

/// Panel that displays detailed information about a single torrent.
///
/// Shows torrent metadata, live stats, and the file list.
pub struct TorrentDetailPanel {
    state: Arc<State>,
    torrent_id: usize,
    details: Option<TorrentDetailsResponse>,
    stats: Option<TorrentStats>,
    file_table_state: Entity<TableState<FileTableDelegate>>,
    focus_handle: FocusHandle,
}

impl EventEmitter<TorrentDetailPanelEvent> for TorrentDetailPanel {}

impl TorrentDetailPanel {
    pub fn new(
        torrent_id: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
        state: Arc<State>,
    ) -> Self {
        let file_table_state = cx
            .new(|cx| TableState::new(FileTableDelegate::new(), window, cx).row_selectable(false));

        let mut this = Self {
            state,
            torrent_id,
            details: None,
            stats: None,
            file_table_state,
            focus_handle: cx.focus_handle(),
        };
        this.fetch_details(cx);
        this
    }

    fn fetch_details(&mut self, cx: &mut Context<Self>) {
        let api = self.state.api();
        let torrent_id = self.torrent_id;
        let file_table_state = self.file_table_state.clone();

        cx.spawn(async move |this, cx| {
            let details = api.api_torrent_details(torrent_id.into());
            let stats = api.api_stats_v1(torrent_id.into());

            let _ = this.update(cx, |this, cx| {
                if let Ok(d) = &details {
                    let files: Vec<FileRow> = d
                        .files
                        .as_ref()
                        .map(|f| {
                            f.iter()
                                .map(|file| FileRow {
                                    name: file.name.clone(),
                                    length: file.length,
                                    included: file.included,
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    let _ = file_table_state.update(cx, |state, cx| {
                        state.delegate_mut().rows = files;
                        cx.notify();
                    });
                }
                this.details = details.ok();
                this.stats = stats.ok();
                cx.notify();
            });
        })
        .detach();
    }

    fn on_back(&mut self, cx: &mut Context<Self>) {
        cx.emit(TorrentDetailPanelEvent::Back);
    }

    fn on_pause(&mut self, cx: &mut Context<Self>) {
        let api = self.state.api();
        let id = self.torrent_id;
        cx.spawn(async move |_, _| {
            let _ = api.api_torrent_action_pause(id.into()).await;
        })
        .detach();
        self.fetch_details(cx);
    }

    fn on_start(&mut self, cx: &mut Context<Self>) {
        let api = self.state.api();
        let id = self.torrent_id;
        cx.spawn(async move |_, _| {
            let _ = api.api_torrent_action_start(id.into()).await;
        })
        .detach();
        self.fetch_details(cx);
    }

    fn on_refresh(&mut self, cx: &mut Context<Self>) {
        self.fetch_details(cx);
    }
}

impl Render for TorrentDetailPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        let name = self
            .details
            .as_ref()
            .and_then(|d| d.name.clone())
            .unwrap_or_else(|| "Loading…".to_string());

        let info_hash = self
            .details
            .as_ref()
            .map(|d| d.info_hash.clone())
            .unwrap_or_default();

        let output_folder = self
            .details
            .as_ref()
            .map(|d| d.output_folder.clone())
            .unwrap_or_default();

        let total_pieces = self.details.as_ref().map(|d| d.total_pieces).unwrap_or(0);

        let state_str = self
            .stats
            .as_ref()
            .map(|s| s.state.to_string())
            .unwrap_or_else(|| "unknown".to_string());

        let progress_str = self.stats.as_ref().map_or("N/A".to_string(), |s| {
            if s.total_bytes > 0 {
                format!(
                    "{:.1}%",
                    (s.progress_bytes as f64 / s.total_bytes as f64) * 100.0
                )
            } else {
                "N/A".to_string()
            }
        });

        let downloaded_str = self
            .stats
            .as_ref()
            .map_or("N/A".to_string(), |s| format_bytes(s.progress_bytes));

        let uploaded_str = self
            .stats
            .as_ref()
            .map_or("N/A".to_string(), |s| format_bytes(s.uploaded_bytes));

        let total_str = self
            .stats
            .as_ref()
            .map_or("N/A".to_string(), |s| format_bytes(s.total_bytes));

        let (down_str, up_str, peers_str, eta_str) = self
            .stats
            .as_ref()
            .and_then(|s| s.live.as_ref())
            .map(|live| {
                let peers = live.snapshot.peer_stats.live.to_string();
                let down = format_speed(live.download_speed.mbps);
                let up = format_speed(live.upload_speed.mbps);
                let eta = live
                    .time_remaining
                    .as_ref()
                    .map(|t| t.to_string())
                    .unwrap_or_else(|| "—".to_string());
                (down, up, peers, eta)
            })
            .unwrap_or((
                "N/A".to_string(),
                "N/A".to_string(),
                "N/A".to_string(),
                "—".to_string(),
            ));

        let error_str = self
            .stats
            .as_ref()
            .and_then(|s| s.error.clone())
            .unwrap_or_default();

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
                        Button::new("back")
                            .label("← Back")
                            .on_click(cx.listener(|this, _, _, cx| this.on_back(cx))),
                    )
                    .child(
                        Button::new("refresh")
                            .label("Refresh")
                            .on_click(cx.listener(|this, _, _, cx| this.on_refresh(cx))),
                    )
                    .child(
                        Button::new("pause")
                            .label("Pause")
                            .on_click(cx.listener(|this, _, _, cx| this.on_pause(cx))),
                    )
                    .child(
                        Button::new("start")
                            .label("Start")
                            .on_click(cx.listener(|this, _, _, cx| this.on_start(cx))),
                    ),
            )
            // Content
            .child(
                v_flex()
                    .flex_1()
                    .min_h_0()
                    .p_4()
                    .gap_4()
                    .overflow_y_scroll()
                    // Title
                    .child(
                        div()
                            .text_size(px(20.))
                            .font_weight(FontWeight::Semibold)
                            .child(name),
                    )
                    // Info section
                    .child(
                        v_flex()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::Semibold)
                                    .child("General"),
                            )
                            .child(info_row("Info Hash", info_hash))
                            .child(info_row("Output Folder", output_folder))
                            .child(info_row("Total Pieces", total_pieces.to_string()))
                            .child(info_row("State", state_str)),
                    )
                    // Stats section
                    .child(
                        v_flex()
                            .gap_2()
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::Semibold)
                                    .child("Statistics"),
                            )
                            .child(info_row("Progress", progress_str))
                            .child(info_row("Downloaded", downloaded_str))
                            .child(info_row("Uploaded", uploaded_str))
                            .child(info_row("Total Size", total_str))
                            .child(info_row("Download Speed", down_str))
                            .child(info_row("Upload Speed", up_str))
                            .child(info_row("Peers", peers_str))
                            .child(info_row("ETA", eta_str)),
                    )
                    // Error (if any)
                    .when(!error_str.is_empty(), |this| {
                        this.child(
                            v_flex()
                                .gap_2()
                                .child(
                                    div()
                                        .text_size(px(14.))
                                        .font_weight(FontWeight::Semibold)
                                        .text_color(theme.danger)
                                        .child("Error"),
                                )
                                .child(
                                    div()
                                        .text_size(px(13.))
                                        .text_color(theme.danger)
                                        .child(error_str),
                                ),
                        )
                    })
                    // Files section
                    .child(
                        v_flex()
                            .gap_2()
                            .min_h_0()
                            .flex_1()
                            .child(
                                div()
                                    .text_size(px(14.))
                                    .font_weight(FontWeight::Semibold)
                                    .child("Files"),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_h(px(200.))
                                    .child(DataTable::new(&self.file_table_state)),
                            ),
                    ),
            )
    }
}

impl Focusable for TorrentDetailPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

/// A single row in the file table.
#[derive(Clone)]
struct FileRow {
    name: String,
    length: u64,
    included: bool,
}

/// Table delegate for the file list.
struct FileTableDelegate {
    rows: Vec<FileRow>,
    columns: Vec<Column>,
}

impl FileTableDelegate {
    fn new() -> Self {
        Self {
            rows: Vec::new(),
            columns: vec![
                Column::new("name", "File Name").width(300.),
                Column::new("size", "Size").width(100.),
                Column::new("included", "Included").width(80.),
            ],
        }
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
            "name" => div().child(row.name.clone()),
            "size" => div().child(format_bytes(row.length)),
            "included" => div().child(if row.included { "✓" } else { "—" }),
            _ => div(),
        }
    }
}

/// Build a label/value info row.
fn info_row(label: impl Into<String>, value: impl Into<String>) -> Div {
    let label = label.into();
    let value = value.into();
    h_flex()
        .gap_2()
        .child(
            div()
                .w(px(140.))
                .text_color(gpui::rgb(0x888888))
                .child(label),
        )
        .child(div().flex_1().child(value))
}

/// Format a byte count in human-readable form (binary units).
fn format_bytes(bytes: u64) -> String {
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

/// Format a speed (given in Mbps) in human-readable form.
fn format_speed(mbps: f64) -> String {
    let bytes = mbps * 1024.0 * 1024.0;
    if bytes >= 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} GB/s", bytes / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024.0 * 1024.0 {
        format!("{:.1} MB/s", bytes / (1024.0 * 1024.0))
    } else if bytes >= 1024.0 {
        format!("{:.1} KB/s", bytes / 1024.0)
    } else {
        format!("{:.0} B/s", bytes)
    }
}

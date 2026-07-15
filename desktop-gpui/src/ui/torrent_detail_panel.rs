use gpui::*;
use gpui_component::{
    ActiveTheme as _,
    button::Button,
    h_flex,
    scroll::ScrollableElement,
    tab::{Tab, TabBar},
    table::{Column, DataTable, TableDelegate, TableState},
    v_flex,
};
use librqbit::TorrentStats;
use librqbit::api::{
    PeerStatsFilter, PeerStatsFilterState, PeerStatsSnapshot, TorrentDetailsResponse,
};
use std::collections::HashSet;
use std::sync::Arc;

use crate::state::State;
use crate::ui::file_table::{FileRow, FileTableDelegate, format_bytes};

/// Events emitted by [`TorrentDetailPanel`] to communicate with the parent view.
#[derive(Clone, Debug)]
pub enum TorrentDetailPanelEvent {
    /// User clicked **Back** – close the detail panel.
    Back,
}

/// Which tab is active in the detail panel.
#[derive(Clone, Copy, PartialEq)]
enum DetailTab {
    Overview,
    Trackers,
    Peers,
    Files,
}

/// Panel that displays detailed information about a single torrent.
///
/// Uses a tabbed layout: Overview, Trackers, Peers, Files.
pub struct TorrentDetailPanel {
    state: Arc<State>,
    torrent_id: usize,
    active_tab: DetailTab,
    details: Option<TorrentDetailsResponse>,
    stats: Option<TorrentStats>,
    trackers: Vec<String>,
    peer_stats: Option<PeerStatsSnapshot>,
    file_table_state: Entity<TableState<FileTableDelegate>>,
    peer_table_state: Entity<TableState<PeerTableDelegate>>,
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
        let panel_weak = cx.entity().downgrade();
        let file_table_state = cx.new(|cx| {
            let mut delegate = FileTableDelegate::new();
            delegate.set_table(cx.entity().downgrade());
            delegate.on_toggle = Some(Arc::new({
                let panel = panel_weak.clone();
                move |file_index, included, cx| {
                    if let Some(panel) = panel.upgrade() {
                        panel.update(cx, |panel, cx| {
                            panel.set_file_included(file_index, included, cx);
                        });
                    }
                }
            }));
            delegate.on_toggle_all = Some(Arc::new({
                let panel = panel_weak.clone();
                move |included, cx| {
                    if let Some(panel) = panel.upgrade() {
                        panel.update(cx, |panel, cx| {
                            panel.set_all_files_included(included, cx);
                        });
                    }
                }
            }));
            TableState::new(delegate, window, cx).row_selectable(false)
        });
        let peer_table_state = cx
            .new(|cx| TableState::new(PeerTableDelegate::new(), window, cx).row_selectable(false));

        let mut this = Self {
            state,
            torrent_id,
            active_tab: DetailTab::Overview,
            details: None,
            stats: None,
            trackers: Vec::new(),
            peer_stats: None,
            file_table_state,
            peer_table_state,
            focus_handle: cx.focus_handle(),
        };
        this.fetch_details(cx);
        this
    }

    /// Switch the panel to display a different torrent, refreshing data.
    pub fn switch_torrent(&mut self, torrent_id: usize, cx: &mut Context<Self>) {
        if self.torrent_id == torrent_id {
            return;
        }
        self.torrent_id = torrent_id;
        self.details = None;
        self.stats = None;
        self.trackers.clear();
        self.peer_stats = None;
        self.fetch_details(cx);
    }

    fn fetch_details(&mut self, cx: &mut Context<Self>) {
        let api = self.state.api();
        let torrent_id = self.torrent_id;
        let file_table_state = self.file_table_state.clone();
        let peer_table_state = self.peer_table_state.clone();

        cx.spawn(async move |this, cx| {
            let details = api.api_torrent_details(torrent_id.into());
            let stats = api.api_stats_v1(torrent_id.into());

            // Fetch trackers from the managed torrent handle
            let trackers: Vec<String> = api
                .mgr_handle(torrent_id.into())
                .ok()
                .map(|h| h.shared().trackers.iter().map(|t| t.to_string()).collect())
                .unwrap_or_default();

            // Fetch peer stats
            let peer_stats = api.api_peer_stats(
                torrent_id.into(),
                PeerStatsFilter {
                    state: PeerStatsFilterState::All,
                },
            );

            let _ = this.update(cx, |this, cx| {
                if let Ok(d) = &details {
                    let files: Vec<FileRow> = d
                        .files
                        .as_ref()
                        .map(|f| {
                            f.iter()
                                .enumerate()
                                .map(|(idx, file)| FileRow {
                                    file_index: idx,
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

                if let Ok(ref ps) = peer_stats {
                    let peer_rows: Vec<PeerRow> = ps
                        .peers
                        .iter()
                        .map(|(addr, p)| PeerRow {
                            address: addr.clone(),
                            state: p.state.to_string(),
                            client: p.client_name.clone().unwrap_or_default(),
                            conn_kind: p.conn_kind.map(|k| k.to_string()).unwrap_or_default(),
                            downloaded: p.counters.fetched_bytes,
                            uploaded: p.counters.uploaded_bytes,
                        })
                        .collect();
                    let _ = peer_table_state.update(cx, |state, cx| {
                        state.delegate_mut().rows = peer_rows;
                        cx.notify();
                    });
                }

                this.details = details.ok();
                this.stats = stats.ok();
                this.trackers = trackers;
                this.peer_stats = peer_stats.ok();
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

    /// Toggle inclusion of a file and persist it via the API.
    fn set_file_included(&mut self, file_index: usize, included: bool, cx: &mut Context<Self>) {
        // Update the local row immediately for responsive UI.
        let _ = self.file_table_state.update(cx, |state, cx| {
            if let Some(row) = state.delegate_mut().rows.get_mut(file_index) {
                row.included = included;
            }
            cx.notify();
        });

        let api = self.state.api();
        let torrent_id = self.torrent_id;
        let file_table_state = self.file_table_state.clone();

        cx.spawn(async move |_this, cx| {
            // Compute the new set of included file indices from the current table state.
            let only_files: HashSet<usize> = file_table_state.read_with(cx, |state, _| {
                state
                    .delegate()
                    .rows
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| r.included)
                    .map(|(idx, _)| idx)
                    .collect::<HashSet<usize>>()
            });

            let _ = api
                .api_torrent_action_update_only_files(torrent_id.into(), &only_files)
                .await;
        })
        .detach();
    }

    /// Set inclusion state for all files and persist it via the API.
    fn set_all_files_included(&mut self, included: bool, cx: &mut Context<Self>) {
        // Update the local rows immediately for responsive UI.
        let _ = self.file_table_state.update(cx, |state, cx| {
            for row in state.delegate_mut().rows.iter_mut() {
                row.included = included;
            }
            cx.notify();
        });

        let api = self.state.api();
        let torrent_id = self.torrent_id;
        let file_table_state = self.file_table_state.clone();

        cx.spawn(async move |_this, cx| {
            let only_files: HashSet<usize> = file_table_state.read_with(cx, |state, _| {
                state
                    .delegate()
                    .rows
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| r.included)
                    .map(|(idx, _)| idx)
                    .collect::<HashSet<usize>>()
            });

            let _ = api
                .api_torrent_action_update_only_files(torrent_id.into(), &only_files)
                .await;
        })
        .detach();
    }

    fn render_overview(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
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
            .gap_4()
            .p_4()
            .overflow_y_scrollbar()
            .child(
                div()
                    .text_size(px(20.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(name),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("General"),
                    )
                    .child(info_row("Info Hash", info_hash))
                    .child(info_row("Output Folder", output_folder))
                    .child(info_row("Total Pieces", total_pieces.to_string()))
                    .child(info_row("State", state_str)),
            )
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(14.))
                            .font_weight(FontWeight::SEMIBOLD)
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
            .children(if error_str.is_empty() {
                None
            } else {
                Some(
                    v_flex()
                        .gap_2()
                        .child(
                            div()
                                .text_size(px(14.))
                                .font_weight(FontWeight::SEMIBOLD)
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
    }

    fn render_trackers(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        if self.trackers.is_empty() {
            return v_flex().p_4().child(
                div()
                    .text_color(gpui::rgb(0x888888))
                    .child("No trackers found."),
            );
        }

        v_flex()
            .p_4()
            .gap_2()
            .child(
                div()
                    .text_size(px(14.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(format!("Trackers ({})", self.trackers.len())),
            )
            .children(self.trackers.iter().map(|t| {
                h_flex()
                    .gap_2()
                    .py_1()
                    .child(div().flex_1().child(t.clone()))
            }))
    }

    fn render_peers(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .p_4()
            .size_full()
            .child(DataTable::new(&self.peer_table_state))
    }

    fn render_files(&self, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .p_4()
            .size_full()
            .child(DataTable::new(&self.file_table_state))
    }
}

impl Render for TorrentDetailPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        let active_tab = self.active_tab;

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
            // Tab bar
            .child(
                TabBar::new("detail-tabs")
                    .underline()
                    .selected_index(match active_tab {
                        DetailTab::Overview => 0,
                        DetailTab::Trackers => 1,
                        DetailTab::Peers => 2,
                        DetailTab::Files => 3,
                    })
                    .on_click(cx.listener(|this, index, _, cx| {
                        this.active_tab = match index {
                            0 => DetailTab::Overview,
                            1 => DetailTab::Trackers,
                            2 => DetailTab::Peers,
                            3 => DetailTab::Files,
                            _ => DetailTab::Overview,
                        };
                        cx.notify();
                    }))
                    .child(Tab::new().label("Overview"))
                    .child(Tab::new().label("Trackers"))
                    .child(Tab::new().label("Peers"))
                    .child(Tab::new().label("Files")),
            )
            // Tab content
            .child(div().flex_1().min_h_0().child(match active_tab {
                DetailTab::Overview => self.render_overview(cx).into_any_element(),
                DetailTab::Trackers => self.render_trackers(cx).into_any_element(),
                DetailTab::Peers => self.render_peers(cx).into_any_element(),
                DetailTab::Files => self.render_files(cx).into_any_element(),
            }))
    }
}

impl Focusable for TorrentDetailPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

// --- Peer table ---

/// A single row in the peer table.
#[derive(Clone)]
struct PeerRow {
    address: String,
    state: String,
    client: String,
    conn_kind: String,
    downloaded: u64,
    uploaded: u64,
}

/// Table delegate for the peer list.
struct PeerTableDelegate {
    rows: Vec<PeerRow>,
    columns: Vec<Column>,
}

impl PeerTableDelegate {
    fn new() -> Self {
        Self {
            rows: Vec::new(),
            columns: vec![
                Column::new("address", "Address").width(180.),
                Column::new("state", "State").width(80.),
                Column::new("client", "Client").width(150.),
                Column::new("conn", "Connection").width(80.),
                Column::new("downloaded", "Downloaded").width(100.),
                Column::new("uploaded", "Uploaded").width(100.),
            ],
        }
    }
}

impl TableDelegate for PeerTableDelegate {
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
            "address" => div().child(row.address.clone()),
            "state" => div().child(row.state.clone()),
            "client" => div().child(row.client.clone()),
            "conn" => div().child(row.conn_kind.clone()),
            "downloaded" => div().child(format_bytes(row.downloaded)),
            "uploaded" => div().child(format_bytes(row.uploaded)),
            _ => div(),
        }
    }
}

// --- Helpers ---

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

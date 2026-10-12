//! Torrent list table: row model, selection/sort state, and cell rendering.

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_kit::component::table::{Column, ColumnSort, TableDelegate, TableState};
use gpui_kit::component::{
    ActiveTheme as _, Icon, IconName, Sizable as _, checkbox::Checkbox, h_flex,
    menu::PopupMenuItem, progress::Progress,
};
use librqbit::TorrentStatsState;
use librqbit::api::TorrentDetailsResponse;
use std::collections::HashSet;

use crate::State;
use crate::ui::main_panel::MainPanel;
use crate::ui::utils::{format_bytes, format_speed, torrent_action_menu_item};

/// Pause/start actions that can be applied to selected torrents.
#[derive(Clone, Copy)]
pub enum TorrentAction {
    Pause,
    Start,
}

/// Visual tone for a torrent row, derived from the torrent state. Drives
/// row-level styling: paused rows render with dimmed text, and the progress
/// bar fills green while downloading and red on error.
#[derive(Clone, Copy, PartialEq)]
pub enum RowTone {
    /// Default appearance.
    Normal,
    /// Paused (or initializing in a paused holding state): dimmed row text.
    Paused,
    /// Live and not yet finished downloading: green progress bar.
    Downloading,
    /// Live and finished (seeding): default progress bar.
    Seeding,
    /// Errored: red progress bar.
    Error,
}

/// Simplified torrent row data that implements Clone.
///
/// Each display field has a corresponding raw numeric field (suffixed `_raw`)
/// that holds the original value from the API. Sorting uses the raw values
/// directly, avoiding lossy re-parsing of formatted strings.
#[derive(Clone)]
pub struct TorrentRow {
    pub id: usize,
    pub name: String,
    pub state: String,
    /// Display string, e.g. "42.1%".
    pub progress: String,
    /// 0.0–100.0
    pub progress_raw: f64,
    pub peers: String,
    /// Derived visual tone used for row styling (dim text, bar colors).
    pub tone: RowTone,
    /// Live peer count (connected), used for sorting.
    pub peers_raw: u32,
    pub down_speed: String,
    /// Download speed in bytes/s.
    pub down_speed_raw: f64,
    pub up_speed: String,
    /// Upload speed in bytes/s.
    pub up_speed_raw: f64,
    pub downloaded: String,
    /// Downloaded bytes.
    pub downloaded_raw: u64,
    pub uploaded: String,
    /// Uploaded bytes.
    pub uploaded_raw: u64,
    pub eta: String,
    /// ETA in seconds; `None` means unknown / not applicable (sorts last).
    pub eta_raw: Option<u64>,
}

impl TorrentRow {
    /// Build a display row from a torrent-list API entry.
    ///
    /// Data is tiered: `id`/`name` are always present, size and progress fields
    /// need `stats`, and peer/speed/ETA fields additionally need live stats.
    /// Fields whose tier is missing get placeholder display strings ("N/A",
    /// "—", "Unknown") with zeroed/`None` raw values so sorting still behaves
    /// sensibly.
    pub fn from_listed(t: &TorrentDetailsResponse) -> Self {
        let stats = t.stats.as_ref();
        let live = stats.and_then(|s| s.live.as_ref());

        // Progress percentage; a torrent with unknown total size counts as 0%.
        let progress_raw = stats.map_or(0.0, |s| {
            if s.total_bytes > 0 {
                s.progress_bytes as f64 / s.total_bytes as f64 * 100.0
            } else {
                0.0
            }
        });

        // Visual tone for row styling. Initializing torrents that are paused
        // count as paused (e.g. watched-folder torrents are added that way).
        let tone = match stats {
            None => RowTone::Normal,
            Some(s) => match s.state {
                TorrentStatsState::Error => RowTone::Error,
                TorrentStatsState::Paused => RowTone::Paused,
                TorrentStatsState::Initializing { paused: true } => RowTone::Paused,
                TorrentStatsState::Initializing { paused: false } => RowTone::Normal,
                TorrentStatsState::Live if !s.finished => RowTone::Downloading,
                TorrentStatsState::Live => RowTone::Seeding,
            },
        };

        Self {
            id: t.id.unwrap_or(0),
            name: t.name.clone().unwrap_or_else(|| t.info_hash.clone()),
            state: stats.map_or_else(|| "Unknown".into(), |s| s.state.to_string()),
            progress: match stats {
                Some(s) if s.total_bytes > 0 => format!("{progress_raw:.1}%"),
                Some(_) => "0%".to_string(),
                None => "N/A".to_string(),
            },
            progress_raw,
            peers: live.map_or_else(
                || "N/A".into(),
                |l| {
                    format!(
                        "{}/{}",
                        l.snapshot.peer_stats.live, l.snapshot.peer_stats.seen
                    )
                },
            ),
            peers_raw: live.map_or(0, |l| l.snapshot.peer_stats.live),
            down_speed: live.map_or_else(|| "N/A".into(), |l| format_speed(l.download_speed.mbps)),
            down_speed_raw: live.map_or(0.0, |l| l.download_speed.as_bytes() as f64),
            tone,
            up_speed: live.map_or_else(|| "N/A".into(), |l| format_speed(l.upload_speed.mbps)),
            up_speed_raw: live.map_or(0.0, |l| l.upload_speed.as_bytes() as f64),
            downloaded: stats.map_or_else(|| "N/A".into(), |s| format_bytes(s.progress_bytes)),
            downloaded_raw: stats.map_or(0, |s| s.progress_bytes),
            uploaded: stats.map_or_else(|| "N/A".into(), |s| format_bytes(s.uploaded_bytes)),
            uploaded_raw: stats.map_or(0, |s| s.uploaded_bytes),
            eta: live.map_or_else(
                || "—".into(),
                |l| {
                    l.time_remaining
                        .as_ref()
                        .map(|t| t.to_string())
                        .unwrap_or_else(|| "—".to_string())
                },
            ),
            eta_raw: live.and_then(|l| l.time_remaining.as_ref().map(|t| t.as_secs())),
        }
    }
}

/// Active search/status filter stored in the delegate; applied to `all_rows`
/// to derive the visible `rows`.
#[derive(Default)]
struct TorrentFilter {
    /// Name substring filter, case-insensitive; empty = no search filter.
    search: String,
    /// Exact match against the torrent state; empty = all statuses.
    status: String,
}

/// Table delegate that holds torrent row data.
pub struct TorrentTableDelegate {
    /// Full, unfiltered set of rows as last fetched from the API. The visible
    /// `rows` are derived from this by [`TorrentTableDelegate::apply_filter`].
    pub all_rows: Vec<TorrentRow>,
    pub rows: Vec<TorrentRow>,
    columns: Vec<Column>,
    /// Currently active search/status filter, pushed in by the panel via
    /// [`TorrentTableDelegate::set_filter`].
    filter: TorrentFilter,
    /// Multi-selection state: set of selected row indices.
    pub selected_rows: HashSet<usize>,
    /// The anchor row for shift-click range selection.
    anchor_row: Option<usize>,
    /// Currently active sort column index (None = no active sort).
    sort_col_ix: Option<usize>,
    /// Currently active sort direction.
    sort_dir: ColumnSort,
    /// Weak handle to the parent [`MainPanel`] so the context menu can open dialogs.
    main_panel: WeakEntity<MainPanel>,
}

impl TorrentTableDelegate {
    pub fn new(main_panel: WeakEntity<MainPanel>) -> Self {
        Self {
            all_rows: Vec::new(),
            rows: Vec::new(),
            columns: vec![
                Column::new("select", "").width(40.),
                Column::new("name", "Name").width(300.).sortable(),
                Column::new("state", "Status").width(100.).sortable(),
                Column::new("progress", "Progress").width(140.).sortable(),
                Column::new("down", "↓ Speed").width(120.).sortable(),
                Column::new("downloaded", "Downloaded")
                    .width(110.)
                    .sortable(),
                Column::new("up", "↑ Speed").width(120.).sortable(),
                Column::new("uploaded", "Uploaded").width(100.).sortable(),
                Column::new("eta", "ETA").width(100.).sortable(),
                Column::new("peers", "Peers").width(90.).sortable(),
            ],
            filter: TorrentFilter::default(),
            selected_rows: HashSet::new(),
            anchor_row: None,
            sort_col_ix: None,
            sort_dir: ColumnSort::Default,
            main_panel,
        }
    }

    /// Store the given search text and status filter, then re-derive the
    /// visible rows. Called whenever the search box or status dropdown
    /// changes.
    pub fn set_filter(&mut self, search: String, status: String) {
        self.filter = TorrentFilter { search, status };
        self.apply_filter();
    }

    /// Recompute the visible `rows` from `all_rows` using the stored filter:
    /// search text matches the name as a case-insensitive substring, status
    /// must match the torrent's `state` exactly. Called after every data
    /// refresh and by [`TorrentTableDelegate::set_filter`], so the view stays
    /// correct even as torrent statuses update live.
    pub fn apply_filter(&mut self) {
        let needle = self.filter.search.trim().to_lowercase();
        let status = self.filter.status.clone();
        self.rows = self
            .all_rows
            .iter()
            .filter(|r| {
                if !needle.is_empty() && !r.name.to_lowercase().contains(&needle) {
                    return false;
                }
                if !status.is_empty() && r.state != status {
                    return false;
                }
                true
            })
            .cloned()
            .collect();
        // Row indices shifted, so any index-based selection is now stale.
        self.selected_rows.clear();
        self.anchor_row = None;
        // Keep the visible rows ordered per the active sort.
        self.apply_sort();
    }

    /// Apply the currently persisted sort (`sort_col_ix` / `sort_dir`) to
    /// `self.rows`. No-op when no sort is active. Used both after a manual
    /// header click and after each data refresh so the sort survives updates.
    fn apply_sort(&mut self) {
        let Some(col_ix) = self.sort_col_ix else {
            return;
        };
        let Some(col) = self.columns.get(col_ix) else {
            return;
        };
        let key = col.key.as_ref().to_string();

        // Ordering comparator for the active column. Larger == "greater".
        // Uses the raw numeric fields directly — no string re-parsing.
        let cmp = |a: &TorrentRow, b: &TorrentRow| -> std::cmp::Ordering {
            match key.as_str() {
                "name" => a.name.cmp(&b.name),
                "state" => a.state.cmp(&b.state),
                "progress" => a
                    .progress_raw
                    .partial_cmp(&b.progress_raw)
                    .unwrap_or(std::cmp::Ordering::Equal),
                "peers" => a.peers_raw.cmp(&b.peers_raw),
                "down" => a.down_speed_raw.total_cmp(&b.down_speed_raw),
                "downloaded" => a.downloaded_raw.cmp(&b.downloaded_raw),
                "up" => a.up_speed_raw.total_cmp(&b.up_speed_raw),
                "uploaded" => a.uploaded_raw.cmp(&b.uploaded_raw),
                "eta" => {
                    // `None` (unknown ETA) sorts last in ascending order.
                    match (a.eta_raw, b.eta_raw) {
                        (Some(a), Some(b)) => a.cmp(&b),
                        (Some(_), None) => std::cmp::Ordering::Less,
                        (None, Some(_)) => std::cmp::Ordering::Greater,
                        (None, None) => std::cmp::Ordering::Equal,
                    }
                }
                _ => std::cmp::Ordering::Equal,
            }
        };

        match self.sort_dir {
            ColumnSort::Default => self.rows.sort_by_key(|r| r.id),
            ColumnSort::Ascending => self.rows.sort_by(cmp),
            ColumnSort::Descending => {
                self.rows.sort_by(cmp);
                self.rows.reverse();
            }
        }
    }
}

impl TableDelegate for TorrentTableDelegate {
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
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let row = &self.rows[row_ix];
        let col = &self.columns[col_ix];

        match col.key.as_ref() {
            "select" => {
                let checked = self.selected_rows.contains(&row_ix);
                let table_state = cx.entity().clone();
                // Fill the cell so the checkbox centers in both axes, matching
                // the header checkbox (a block child would otherwise only
                // stretch horizontally and stay top-aligned).
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    // Prevent the row's mouse-down handler from hijacking the toggle.
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        Checkbox::new(("select-cb", row_ix))
                            .checked(checked)
                            .on_click(move |new_checked, _, cx| {
                                table_state.update(cx, |state, cx| {
                                    if *new_checked {
                                        state.delegate_mut().selected_rows.insert(row_ix);
                                    } else {
                                        state.delegate_mut().selected_rows.remove(&row_ix);
                                    }
                                    cx.notify();
                                });
                            }),
                    )
            }
            "name" => div().child(row.name.clone()),
            "state" => {
                // Status icon tinted by tone: green arrows while live (down
                // while downloading, up when seeding after completion), a
                // dimmed pause icon when paused (matching the row's dimmed
                // text), and red on error.
                let theme = cx.theme();
                let icon = match row.tone {
                    RowTone::Downloading => {
                        Some(Icon::new(IconName::ArrowDown).text_color(theme.blue_light))
                    }
                    RowTone::Seeding => {
                        Some(Icon::new(IconName::ArrowUp).text_color(theme.success))
                    }
                    RowTone::Paused => {
                        Some(Icon::new(IconName::Pause).text_color(theme.muted_foreground))
                    }
                    RowTone::Error => Some(Icon::new(IconName::CircleX).text_color(theme.danger)),
                    RowTone::Normal => None,
                };
                h_flex()
                    .gap_1()
                    .when_some(icon, |this, icon| this.child(icon.small()))
                    .child(div().child(row.state.clone()))
            }
            "progress" => {
                // Green while downloading, red on error; other tones keep the
                // theme's default fill.
                let bar = match row.tone {
                    RowTone::Error => Progress::new(("progress-bar", row_ix))
                        .value(row.progress_raw as f32)
                        .color(cx.theme().danger),
                    RowTone::Downloading => Progress::new(("progress-bar", row_ix))
                        .value(row.progress_raw as f32)
                        .color(cx.theme().blue_light),
                    _ => Progress::new(("progress-bar", row_ix)).value(row.progress_raw as f32),
                };
                h_flex()
                    .justify_between()
                    .gap_2()
                    .child(div().child(bar).min_w(px(70.)))
                    .child(div().child(row.progress.clone()))
            }
            "peers" => div().child(row.peers.clone()),
            "down" => div().child(row.down_speed.clone()),
            "downloaded" => div().child(row.downloaded.clone()),
            "up" => div().child(row.up_speed.clone()),
            "uploaded" => div().child(row.uploaded.clone()),
            "eta" => div().child(row.eta.clone()),
            _ => div(),
        }
    }

    fn render_th(
        &mut self,
        col_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let col = &self.columns[col_ix];
        if col.key.as_ref() == "select" {
            let all_selected = !self.rows.is_empty() && self.selected_rows.len() == self.rows.len();
            let total = self.rows.len();
            let table_state = cx.entity().clone();
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Checkbox::new("select-all-cb")
                        .checked(all_selected)
                        .on_click(move |new_checked, _, cx| {
                            table_state.update(cx, |state, cx| {
                                if *new_checked {
                                    state.delegate_mut().selected_rows = (0..total).collect();
                                } else {
                                    state.delegate_mut().selected_rows.clear();
                                }
                                cx.notify();
                            });
                        }),
                )
                .into_any_element();
        }
        div()
            .size_full()
            .child(self.column(col_ix, cx).name.clone())
            .into_any_element()
    }

    /// Sort the torrent rows by the clicked column.
    ///
    /// Triggered by the table's built-in sort icon (rendered for any column that
    /// is `.sortable()`). `sort` is the new direction the table is cycling to:
    /// `Ascending` -> `Default` (unsorted, restored to API id order) ->
    /// `Descending`.
    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: ColumnSort,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        // Persist the active sort so it survives subsequent data refreshes
        // (which replace `self.rows` wholesale).
        self.sort_col_ix = Some(col_ix);
        self.sort_dir = sort;
        self.apply_sort();

        // Row indices shifted, so any index-based selection is now stale.
        self.selected_rows.clear();
        self.anchor_row = None;
        cx.notify();
    }

    fn render_tr(
        &mut self,
        row_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> Stateful<Div> {
        let is_selected = self.selected_rows.contains(&row_ix);
        // Paused torrents render with dimmed text; the color inherits into
        // every cell of the row. Hoisted out of the style chain so the
        // closure doesn't need to borrow `cx`.
        let is_paused = self
            .rows
            .get(row_ix)
            .is_some_and(|r| r.tone == RowTone::Paused);
        let dimmed_text = cx.theme().muted_foreground;
        div()
            .id(("torrent-row", row_ix))
            .when(is_paused, |this| this.text_color(dimmed_text))
            .when(is_selected, |this| this.bg(gpui::rgb(0x2a2a3e)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |state, e: &MouseDownEvent, _, cx| {
                    let delegate = state.delegate_mut();
                    if e.modifiers.shift {
                        let start = delegate.anchor_row.unwrap_or(row_ix);
                        let (lo, hi) = if start <= row_ix {
                            (start, row_ix)
                        } else {
                            (row_ix, start)
                        };
                        delegate.selected_rows.clear();
                        for i in lo..=hi {
                            delegate.selected_rows.insert(i);
                        }
                    } else if e.modifiers.control {
                        if delegate.selected_rows.contains(&row_ix) {
                            delegate.selected_rows.remove(&row_ix);
                        } else {
                            delegate.selected_rows.insert(row_ix);
                        }
                        delegate.anchor_row = Some(row_ix);
                    } else {
                        delegate.selected_rows.clear();
                        delegate.selected_rows.insert(row_ix);
                        delegate.anchor_row = Some(row_ix);
                    }
                    cx.notify();
                }),
            )
    }

    fn context_menu(
        &mut self,
        row_ix: usize,
        menu: gpui_kit::component::menu::PopupMenu,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> gpui_kit::component::menu::PopupMenu {
        let Some(row) = self.rows.get(row_ix) else {
            return menu;
        };
        let torrent_id = row.id;
        let torrent_name = row.name.clone();

        // Select the right-clicked row so actions target it.
        self.selected_rows.clear();
        self.selected_rows.insert(row_ix);
        self.anchor_row = Some(row_ix);

        // Clone the output folder for the open-folder handler.
        // api_torrent_details is synchronous, so we can call it here.
        let api = _cx.global::<State>().api();
        let output_folder = api
            .api_torrent_details(torrent_id.into())
            .ok()
            .map(|d| d.output_folder)
            .unwrap_or_default();

        menu.item(torrent_action_menu_item(
            format!("Pause: {torrent_name}"),
            torrent_id,
            TorrentAction::Pause,
        ))
        .item(torrent_action_menu_item(
            format!("Start: {torrent_name}"),
            torrent_id,
            TorrentAction::Start,
        ))
        .separator()
        .item(
            PopupMenuItem::new("Open Folder").on_click(move |_, _, _cx: &mut App| {
                if !output_folder.is_empty() {
                    let _ = std::process::Command::new("open")
                        .arg(&output_folder)
                        .spawn();
                }
            }),
        )
        .separator()
        .item(
            PopupMenuItem::new(format!("Delete: {torrent_name}")).on_click({
                let main_panel = self.main_panel.clone();
                move |_, window, cx: &mut App| {
                    if let Some(main) = main_panel.upgrade() {
                        main.update(cx, |this, cx| {
                            this.open_delete_dialog(vec![torrent_id], window, cx);
                        });
                    }
                }
            }),
        )
    }
}

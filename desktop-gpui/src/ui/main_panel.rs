use gpui::*;
use gpui_component::{
    ActiveTheme as _,
    button::Button,
    h_flex,
    table::{Column, DataTable, TableDelegate, TableState},
    v_flex,
};
use librqbit::api::ApiTorrentListOpts;
use std::sync::Arc;

use crate::state::State;
use crate::ui::config_modal::{ConfigModal, ConfigModalEvent};
use crate::ui::torrent_detail_panel::{TorrentDetailPanel, TorrentDetailPanelEvent};

/// Simplified torrent row data that implements Clone.
#[derive(Clone)]
struct TorrentRow {
    id: usize,
    name: String,
    info_hash: String,
    state: String,
    progress: String,
    peers: String,
    down_speed: String,
    up_speed: String,
}

/// Main panel that displays the list of torrents.
pub struct MainPanel {
    state: Arc<State>,
    table_state: Entity<TableState<TorrentTableDelegate>>,
    config_modal: Option<Entity<ConfigModal>>,
    detail_panel: Option<Entity<TorrentDetailPanel>>,
    focus_handle: FocusHandle,
}

impl MainPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.global::<State>().clone();

        let table_state = cx.new(|cx| {
            TableState::new(TorrentTableDelegate::new(), window, cx).row_selectable(true)
        });

        let mut this = Self {
            state: Arc::new(state),
            table_state,
            config_modal: None,
            detail_panel: None,
            focus_handle: cx.focus_handle(),
        };

        this.fetch_torrents(cx);
        this
    }

    fn fetch_torrents(&mut self, cx: &mut Context<Self>) {
        let api = self.state.api();
        let table_state = self.table_state.clone();
        cx.spawn(async move |this, cx| {
            let response = api.api_torrent_list_ext(ApiTorrentListOpts { with_stats: true });
            let rows: Vec<TorrentRow> = response
                .torrents
                .into_iter()
                .map(|t| {
                    let id = t.id.unwrap_or(0);
                    let name = t.name.clone().unwrap_or_else(|| t.info_hash.clone());
                    let info_hash = t.info_hash.clone();

                    let (state_str, progress_str, peers_str, down_str, up_str) =
                        if let Some(stats) = &t.stats {
                            let st = stats.state.to_string();
                            let prog = if stats.total_bytes > 0 {
                                let pct = (stats.progress_bytes as f64 / stats.total_bytes as f64)
                                    * 100.0;
                                format!("{:.1}%", pct)
                            } else {
                                "0%".to_string()
                            };
                            let (peers, down, up) = if let Some(live) = &stats.live {
                                (
                                    live.snapshot.peer_stats.live.to_string(),
                                    format_speed(live.download_speed.mbps),
                                    format_speed(live.upload_speed.mbps),
                                )
                            } else {
                                ("N/A".to_string(), "N/A".to_string(), "N/A".to_string())
                            };
                            (st, prog, peers, down, up)
                        } else {
                            (
                                "Unknown".to_string(),
                                "N/A".to_string(),
                                "N/A".to_string(),
                                "N/A".to_string(),
                                "N/A".to_string(),
                            )
                        };

                    TorrentRow {
                        id,
                        name,
                        info_hash,
                        state: state_str,
                        progress: progress_str,
                        peers: peers_str,
                        down_speed: down_str,
                        up_speed: up_str,
                    }
                })
                .collect();

            let _ = table_state.update(cx, |state, cx| {
                state.delegate_mut().rows = rows;
                cx.notify();
            });

            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }

    /// Get the selected torrent's ID (single-row selection).
    fn selected_torrent_id(&self, cx: &mut Context<Self>) -> Option<usize> {
        let table = self.table_state.read(cx);
        table
            .selected_row()
            .and_then(|r| table.delegate().rows.get(r).map(|row| row.id))
    }

    fn on_pause(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.selected_torrent_id(cx) {
            let api = self.state.api();
            cx.spawn(async move |_, _| {
                let _ = api.api_torrent_action_pause(id.into()).await;
            })
            .detach();
            self.fetch_torrents(cx);
        }
    }

    fn on_start(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.selected_torrent_id(cx) {
            let api = self.state.api();
            cx.spawn(async move |_, _| {
                let _ = api.api_torrent_action_start(id.into()).await;
            })
            .detach();
            self.fetch_torrents(cx);
        }
    }

    fn on_delete(&mut self, cx: &mut Context<Self>) {
        if let Some(id) = self.selected_torrent_id(cx) {
            let api = self.state.api();
            cx.spawn(async move |_, _| {
                let _ = api.api_torrent_action_delete(id.into()).await;
            })
            .detach();
            self.fetch_torrents(cx);
        }
    }

    fn on_refresh(&mut self, cx: &mut Context<Self>) {
        self.fetch_torrents(cx);
    }

    fn on_details(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.selected_torrent_id(cx) {
            let state = self.state.clone();
            let panel = cx.new(|cx| {
                let panel = TorrentDetailPanel::new(id, window, cx, state);
                cx.subscribe(
                    &cx.entity(),
                    |this, _entity, event: &TorrentDetailPanelEvent, cx| match event {
                        TorrentDetailPanelEvent::Back => this.close_details(cx),
                    },
                )
                .detach();
                panel
            });
            self.detail_panel = Some(panel);
            cx.notify();
        }
    }

    fn close_details(&mut self, cx: &mut Context<Self>) {
        self.detail_panel = None;
        self.fetch_torrents(cx);
        cx.notify();
    }

    fn on_settings(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let modal = cx.new(|cx| ConfigModal::new(_window, cx, self.state.clone()));
        cx.subscribe(
            &modal,
            |this, _entity, event: &ConfigModalEvent, cx| match event {
                ConfigModalEvent::Applied | ConfigModalEvent::Cancelled => {
                    this.close_settings(cx);
                }
            },
        )
        .detach();
        self.config_modal = Some(modal);
        cx.notify();
    }

    fn close_settings(&mut self, cx: &mut Context<Self>) {
        self.config_modal = None;
        cx.notify();
    }
}

/// Table delegate that holds torrent row data.
struct TorrentTableDelegate {
    rows: Vec<TorrentRow>,
    columns: Vec<Column>,
}

impl TorrentTableDelegate {
    fn new() -> Self {
        Self {
            rows: Vec::new(),
            columns: vec![
                Column::new("id", "ID").width(50.),
                Column::new("name", "Name").width(200.),
                Column::new("state", "Status").width(80.),
                Column::new("progress", "Progress").width(80.),
                Column::new("peers", "Peers").width(60.),
                Column::new("down", "Down Speed").width(100.),
                Column::new("up", "Up Speed").width(100.),
            ],
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
        _: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let row = &self.rows[row_ix];
        let col = &self.columns[col_ix];

        match col.key.as_ref() {
            "id" => div().child(row.id.to_string()),
            "name" => div().child(row.name.clone()),
            "state" => div().child(row.state.clone()),
            "progress" => div().child(row.progress.clone()),
            "peers" => div().child(row.peers.clone()),
            "down" => div().child(row.down_speed.clone()),
            "up" => div().child(row.up_speed.clone()),
            _ => div(),
        }
    }
}

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

impl Render for MainPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        v_flex()
            .size_full()
            .gap_0()
            .child(
                // Toolbar (extra left padding for macOS traffic lights)
                h_flex()
                    .gap_2()
                    .pl(px(78.))
                    .pr_2()
                    .py_2()
                    .bg(theme.background)
                    .border_b_1()
                    .border_color(theme.border)
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
                    )
                    .child(
                        Button::new("delete")
                            .label("Delete")
                            .on_click(cx.listener(|this, _, _, cx| this.on_delete(cx))),
                    )
                    .child(
                        Button::new("details").label("Details").on_click(
                            cx.listener(|this, _, window, cx| this.on_details(window, cx)),
                        ),
                    )
                    .child(
                        Button::new("settings").label("Settings").on_click(
                            cx.listener(|this, _, window, cx| this.on_settings(window, cx)),
                        ),
                    ),
            )
            .child(
                // Torrent table or detail panel
                if let Some(detail) = &self.detail_panel {
                    div().size_full().child(detail.clone())
                } else {
                    div().size_full().child(DataTable::new(&self.table_state))
                },
            )
            .children(self.config_modal.as_ref().map(|modal| {
                let theme = cx.theme();
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .bg(theme.muted)
                    .opacity(0.8)
                    .child(
                        v_flex()
                            .absolute()
                            .top(px(20.))
                            .left(px(20.))
                            .right(px(20.))
                            .bottom(px(20.))
                            .bg(theme.background)
                            .rounded_md()
                            .border_1()
                            .border_color(theme.border)
                            .shadow_lg()
                            .p_4()
                            .overflow_hidden()
                            .child(modal.clone())
                            // Prevent clicks inside the modal from closing it.
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this: &mut MainPanel, _, _, cx| {
                            this.close_settings(cx);
                        }),
                    )
            }))
    }
}

impl Focusable for MainPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

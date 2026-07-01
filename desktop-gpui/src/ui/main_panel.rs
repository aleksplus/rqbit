use gpui::*;
use gpui_component::{
    table::{Column, DataTable, TableState},
    button::Button,
    ActiveTheme as _, StyledExt as _, h_flex, v_flex,
};
use librqbit::api::{ApiTorrentListOpts, TorrentDetailsResponse};
use std::sync::Arc;

use crate::state::{State};

/// Main panel that displays the list of torrents.
pub struct MainPanel {
    state: Arc<State>,
    torrents: Vec<TorrentDetailsResponse>,
    table_state: Entity<TableState<MainPanel>>,
    columns: Vec<Column>,
    focus_handle: FocusHandle,
}

impl MainPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.global::<State>().clone();

        // Create columns
        let columns = vec![
            Column::new("ID", "ID"),
            Column::new("Name", "Name"),
            Column::new("Status", "Status"),
            Column::new("Progress", "Progress"),
            Column::new("Peers", "Peers"),
            Column::new("Down Speed", "Down Speed"),
            Column::new("Up Speed", "Up Speed"),
        ];

        let table_state = cx.new(|cx| TableState::new(Self::delegate(), window, cx));

        let mut this = Self {
            state,
            torrents: Vec::new(),
            table_state,
            columns,
            focus_handle: cx.focus_handle(),
        };

        this.fetch_torrents(cx);
        this
    }

    fn delegate() -> TorrentTableDelegate {
        TorrentTableDelegate
    }

    fn fetch_torrents(&mut self, cx: &mut Context<Self>) {
        let api = self.state.api();
        cx.spawn(async move |this, cx| {
            let response = api.api_torrent_list_ext(ApiTorrentListOpts { with_stats: true });
            let torrents = response.torrents;

            let _ = this.update(cx, |view, cx| {
                view.torrents = torrents;
                view.update_table(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn update_table(&self, cx: &mut Context<Self>) {
        self.table_state.update(cx, |state, cx| {
            state.set_rows(self.torrents.clone());
        });
    }

    fn on_pause(&mut self, cx: &mut Context<Self>) {
        let selected = self.table_state.read(cx).selected_rows().cloned().collect::<Vec<_>>();
        if selected.is_empty() {
            return;
        }

        let api = self.state.api();
        for idx in selected {
            if let Some(torrent) = self.torrents.iter().find(|t| t.id == Some(idx)) {
                let api = api.clone();
                let hash = torrent.info_hash.clone();
                cx.spawn(async move |_, _| {
                    let _ = api.api_torrent_action_pause(hash.into()).await;
                })
                .detach();
            }
        }
        self.fetch_torrents(cx);
    }

    fn on_start(&mut self, cx: &mut Context<Self>) {
        let selected = self.table_state.read(cx).selected_rows().cloned().collect::<Vec<_>>();
        if selected.is_empty() {
            return;
        }

        let api = self.state.api();
        for idx in selected {
            if let Some(torrent) = self.torrents.iter().find(|t| t.id == Some(idx)) {
                let api = api.clone();
                let hash = torrent.info_hash.clone();
                cx.spawn(async move |_, _| {
                    let _ = api.api_torrent_action_start(hash.into()).await;
                })
                .detach();
            }
        }
        self.fetch_torrents(cx);
    }

    fn on_delete(&mut self, cx: &mut Context<Self>) {
        let selected = self.table_state.read(cx).selected_rows().cloned().collect::<Vec<_>>();
        if selected.is_empty() {
            return;
        }

        let api = self.state.api();
        for idx in selected {
            if let Some(torrent) = self.torrents.iter().find(|t| t.id == Some(idx)) {
                let api = api.clone();
                let hash = torrent.info_hash.clone();
                cx.spawn(async move |_, _| {
                    let _ = api.api_torrent_action_delete(hash.into()).await;
                })
                .detach();
            }
        }
        self.fetch_torrents(cx);
    }

    fn on_refresh(&mut self, cx: &mut Context<Self>) {
        self.fetch_torrents(cx);
    }
}

#[derive(Clone, Copy)]
struct TorrentTableDelegate;

impl gpui_component::table::TableDelegate for TorrentTableDelegate {
    type Row = TorrentDetailsResponse;

    fn columns_count(&self, _: &App) -> usize {
        7
    }

    fn rows_count(&self, state: &TableState<Self>, _: &App) -> usize {
        state.rows().len()
    }

    fn column(&self, index: usize, _: &App) -> Column {
        match index {
            0 => Column::new("ID", "ID"),
            1 => Column::new("Name", "Name"),
            2 => Column::new("Status", "Status"),
            3 => Column::new("Progress", "Progress"),
            4 => Column::new("Peers", "Peers"),
            5 => Column::new("Down Speed", "Down Speed"),
            6 => Column::new("Up Speed", "Up Speed"),
            _ => Column::new("", ""),
        }
    }

    fn render_td(
        &mut self,
        row: usize,
        col: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let rows = cx.read().rows();
        let torrent = &rows[row];

        match col {
            0 => torrent.id.unwrap_or(0).to_string().into_element(),
            1 => torrent.name.clone().unwrap_or_default().into_element(),
            2 => {
                if let Some(stats) = &torrent.stats {
                    stats.state.to_string().into_element()
                } else {
                    "Unknown".into_element()
                }
            }
            3 => {
                if let Some(stats) = &torrent.stats {
                    if stats.total_bytes > 0 {
                        let percent = (stats.progress_bytes as f64 / stats.total_bytes as f64) * 100.0;
                        format!("{:.1}%", percent).into_element()
                    } else {
                        "0%".into_element()
                    }
                } else {
                    "N/A".into_element()
                }
            }
            4 => {
                if let Some(stats) = &torrent.stats {
                    if let Some(live) = &stats.live {
                        live.snapshot.peers_connected.to_string().into_element()
                    } else {
                        "N/A".into_element()
                    }
                } else {
                    "N/A".into_element()
                }
            }
            5 => {
                if let Some(stats) = &torrent.stats {
                    if let Some(live) = &stats.live {
                        format_bytes(live.download_speed.0 as u64).into_element()
                    } else {
                        "N/A".into_element()
                    }
                } else {
                    "N/A".into_element()
                }
            }
            6 => {
                if let Some(stats) = &torrent.stats {
                    if let Some(live) = &stats.live {
                        format_bytes(live.upload_speed.0 as u64).into_element()
                    } else {
                        "N/A".into_element()
                    }
                } else {
                    "N/A".into_element()
                }
            }
            _ => "".into_element(),
        }
    }
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB/s", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB/s", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB/s", bytes as f64 / KB as f64)
    } else {
        format!("{} B/s", bytes)
    }
}

impl Render for MainPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .gap_2()
            .child(
                // Toolbar
                h_flex()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .bg(cx.theme().background)
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        Button::new("refresh")
                            .label("Refresh")
                            .on_click(cx.listener(|this, _, cx| this.on_refresh(cx))),
                    )
                    .child(
                        Button::new("pause")
                            .label("Pause")
                            .on_click(cx.listener(|this, _, cx| this.on_pause(cx))),
                    )
                    .child(
                        Button::new("start")
                            .label("Start")
                            .on_click(cx.listener(|this, _, cx| this.on_start(cx))),
                    )
                    .child(
                        Button::new("delete")
                            .label("Delete")
                            .on_click(cx.listener(|this, _, cx| this.on_delete(cx))),
                    ),
            )
            .child(
                // Torrent table
                DataTable::new(&self.table_state)
                    .columns(self.columns.clone())
                    .rows(self.torrents.clone())
                    .size_full(),
            )
    }
}

impl Focusable for MainPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
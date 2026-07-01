use gpui::*;
use gpui_component::{
    table::{Column, DataTable, TableDelegate, TableState},
    ActiveTheme as _, Button, Modal, StyledExt as _, h_flex, v_flex,
};
use librqbit::api::{ApiTorrentListOpts, TorrentDetailsResponse};
use std::sync::Arc;

use crate::{
    state::{SharedState, State},
    ui::config_modal::ConfigModal,
};

/// Main panel that displays the list of torrents.
pub struct MainPanel {
    state: Arc<State>,
    torrents: Vec<TorrentDetailsResponse>,
    table_state: Entity<TableState<MainPanel>>,
    columns: Vec<Column<TorrentDetailsResponse>>,
    focus_handle: FocusHandle,
}

impl MainPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        // Get the state from global context
        let state = cx.global::<State>().clone();

        // Create columns
        let columns = vec![
            Column::new("ID", |t: &TorrentDetailsResponse| {
                t.id.unwrap_or(0).to_string()
            }),
            Column::new("Name", |t: &TorrentDetailsResponse| {
                t.name.clone().unwrap_or_default()
            }),
            Column::new("Status", |t: &TorrentDetailsResponse| {
                t.status.clone().unwrap_or_default()
            }),
            Column::new("Progress", |t: &TorrentDetailsResponse| {
                if let Some(stats) = &t.stats {
                    if stats.total_pieces > 0 {
                        let percent = (stats.pieces_completed as f64 / stats.total_pieces as f64) * 100.0;
                        format!("{:.1}%", percent)
                    } else {
                        "0%".to_string()
                    }
                } else {
                    "N/A".to_string()
                }
            }),
            Column::new("Peers", |t: &TorrentDetailsResponse| {
                if let Some(stats) = &t.stats {
                    stats.peers_connected.to_string()
                } else {
                    "N/A".to_string()
                }
            }),
            Column::new("Down Speed", |t: &TorrentDetailsResponse| {
                if let Some(stats) = &t.stats {
                    format_bytes(stats.download_speed)
                } else {
                    "N/A".to_string()
                }
            }),
            Column::new("Up Speed", |t: &TorrentDetailsResponse| {
                if let Some(stats) = &t.stats {
                    format_bytes(stats.upload_speed)
                } else {
                    "N/A".to_string()
                }
            }),
        ];

        let table_state = cx.new(|cx| TableState::new(Self::delegate(), window, cx));

        let mut this = Self {
            state,
            torrents: Vec::new(),
            table_state,
            columns,
            focus_handle: cx.focus_handle(),
        };

        // Fetch initial torrent list
        this.fetch_torrents(cx);

        this
    }

    fn delegate() -> Box<dyn TableDelegate<TorrentDetailsResponse>> {
        Box::new(TorrentTableDelegate)
    }

    fn fetch_torrents(&mut self, cx: &mut Context<Self>) {
        let api = self.state.api();
        cx.spawn(async move |this, cx| {
            let response = api.api_torrent_list_ext(ApiTorrentListOpts { with_stats: true });
            let torrents = response.torrents;

            let _ = this.update(cx, |view, cx| {
                view.torrents = torrents;
                view.table_state.update(cx, |state, cx| {
                    state.set_rows(view.torrents.clone());
                });
                cx.notify();
            });
        })
        .detach();
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

    fn on_add_torrent(&mut self, cx: &mut Context<Self>) {
        // TODO: Implement add torrent dialog
        eprintln!("Add torrent not implemented yet");
    }

    fn on_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let state = self.state.clone();
        window.open_modal(cx, move |window, cx| ConfigModal::new(window, cx, state));
    }

    fn on_refresh(&mut self, cx: &mut Context<Self>) {
        self.fetch_torrents(cx);
    }
}

struct TorrentTableDelegate;

impl TableDelegate<TorrentDetailsResponse> for TorrentTableDelegate {
    fn render_row(&self, row: &TorrentDetailsResponse, _cx: &mut Context<TableState<MainPanel>>) -> impl IntoElement {
        h_flex()
            .gap_2()
            .px_2()
            .py_1()
            .child(div().w(px(50.)).child(row.id.unwrap_or(0).to_string()))
            .child(div().flex_1().child(row.name.clone().unwrap_or_default()))
            .child(div().w(px(100.)).child(row.status.clone().unwrap_or_default()))
            .child(div().w(px(80.)).child({
                if let Some(stats) = &row.stats {
                    if stats.total_pieces > 0 {
                        let percent = (stats.pieces_completed as f64 / stats.total_pieces as f64) * 100.0;
                        format!("{:.1}%", percent)
                    } else {
                        "0%".to_string()
                    }
                } else {
                    "N/A".to_string()
                }
            }))
            .child(div().w(px(60.)).child({
                if let Some(stats) = &row.stats {
                    stats.peers_connected.to_string()
                } else {
                    "N/A".to_string()
                }
            }))
            .child(div().w(px(100.)).child({
                if let Some(stats) = &row.stats {
                    format_bytes(stats.download_speed)
                } else {
                    "N/A".to_string()
                }
            }))
            .child(div().w(px(100.)).child({
                if let Some(stats) = &row.stats {
                    format_bytes(stats.upload_speed)
                } else {
                    "N/A".to_string()
                }
            }))
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
                        Button::new("add")
                            .label("Add Torrent")
                            .on_click(cx.listener(|this, _, cx| this.on_add_torrent(cx))),
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
                            .variant(gpui_component::button::ButtonVariant::Destructive)
                            .on_click(cx.listener(|this, _, cx| this.on_delete(cx))),
                    )
                    .child(h_flex().flex_1()) // spacer
                    .child(
                        Button::new("config")
                            .label("Config")
                            .on_click(cx.listener(move |this, _, cx| {
                                this.on_config(window, cx);
                            })),
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
    fn focus_handle(&self) -> FocusHandle {
        self.focus_handle.clone()
    }
}
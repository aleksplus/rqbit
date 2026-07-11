use gpui::*;
use gpui_component::{
    ActiveTheme as _,
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputState},
    resizable::{ResizableState, h_resizable, resizable_panel},
    table::{Column, DataTable, TableDelegate, TableState},
    v_flex,
};
use librqbit::api::ApiTorrentListOpts;
use librqbit::{AddTorrent, AddTorrentOptions};
use std::sync::Arc;

use crate::state::State;
use crate::ui::settings_page::{SettingsPage, SettingsPageEvent};
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
    resizable_state: Entity<ResizableState>,
    config_modal: Option<Entity<SettingsPage>>,
    detail_panel: Option<Entity<TorrentDetailPanel>>,
    magnet_dialog: Option<Entity<MagnetDialog>>,
    focus_handle: FocusHandle,
}

impl MainPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.global::<State>().clone();

        let table_state = cx.new(|cx| {
            TableState::new(TorrentTableDelegate::new(), window, cx).row_selectable(true)
        });
        let resizable_state = cx.new(|_cx| ResizableState::default());

        let mut this = Self {
            state: Arc::new(state),
            table_state,
            resizable_state,
            config_modal: None,
            detail_panel: None,
            magnet_dialog: None,
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

    fn on_add_torrent_files(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Select .torrent files".into()),
        });

        let api = self.state.api();
        cx.spawn(async move |this, cx| {
            match receiver.await {
                Ok(Ok(Some(paths))) => {
                    for path in paths {
                        match std::fs::read(&path) {
                            Ok(bytes) => {
                                let add = AddTorrent::from_bytes(bytes);
                                if let Err(e) =
                                    api.api_add_torrent(add, None::<AddTorrentOptions>).await
                                {
                                    eprintln!("Error adding torrent {:?}: {:?}", path, e);
                                }
                            }
                            Err(e) => {
                                eprintln!("Error reading file {:?}: {:?}", path, e);
                            }
                        }
                    }
                    let _ = this.update(cx, |this, cx| this.fetch_torrents(cx));
                }
                Ok(Ok(None)) => {} // user cancelled
                Ok(Err(e)) => eprintln!("File dialog error: {:?}", e),
                Err(e) => eprintln!("File dialog receiver error: {:?}", e),
            }
        })
        .detach();
    }

    fn on_add_magnet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dialog = cx.new(|cx| MagnetDialog::new(window, cx));
        cx.subscribe(
            &dialog,
            |this, _entity, event: &MagnetDialogEvent, cx| match event {
                MagnetDialogEvent::Cancelled => this.close_magnet_dialog(cx),
                MagnetDialogEvent::Submitted(magnet) => {
                    this.close_magnet_dialog(cx);
                    this.add_magnet(magnet.clone(), cx);
                }
            },
        )
        .detach();
        self.magnet_dialog = Some(dialog);
        cx.notify();
    }

    fn add_magnet(&mut self, magnet: String, cx: &mut Context<Self>) {
        let api = self.state.api();
        cx.spawn(async move |this, cx| {
            let add = AddTorrent::from_url(magnet);
            if let Err(e) = api.api_add_torrent(add, None::<AddTorrentOptions>).await {
                eprintln!("Error adding magnet: {:?}", e);
            }
            let _ = this.update(cx, |this, cx| this.fetch_torrents(cx));
        })
        .detach();
    }

    fn close_magnet_dialog(&mut self, cx: &mut Context<Self>) {
        self.magnet_dialog = None;
        cx.notify();
    }

    fn on_details(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.selected_torrent_id(cx) {
            let state = self.state.clone();
            let panel = cx.new(|cx| TorrentDetailPanel::new(id, window, cx, state));
            cx.subscribe(
                &panel,
                |this, _entity, event: &TorrentDetailPanelEvent, cx| match event {
                    TorrentDetailPanelEvent::Back => this.close_details(cx),
                },
            )
            .detach();
            self.detail_panel = Some(panel);
            cx.notify();
        }
    }

    fn close_details(&mut self, cx: &mut Context<Self>) {
        // Deselect the table row so the detail panel stays hidden.
        let _ = self.table_state.update(cx, |state, cx| {
            state.clear_selection(cx);
            cx.notify();
        });
        self.detail_panel = None;
        cx.notify();
    }

    fn on_settings(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let page = cx.new(|cx| SettingsPage::new(_window, cx, self.state.clone()));
        cx.subscribe(
            &page,
            |this, _entity, event: &SettingsPageEvent, cx| match event {
                SettingsPageEvent::Back => this.close_settings(cx),
            },
        )
        .detach();
        self.config_modal = Some(page);
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
                    .child(Button::new("add-torrent").label("Add Torrent…").on_click(
                        cx.listener(|this, _, window, cx| this.on_add_torrent_files(window, cx)),
                    ))
                    .child(Button::new("add-magnet").label("Add Magnet…").on_click(
                        cx.listener(|this, _, window, cx| this.on_add_magnet(window, cx)),
                    ))
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
            .child(if let Some(settings) = &self.config_modal {
                // Settings page replaces the main content
                div().size_full().child(settings.clone()).into_any_element()
            } else {
                // Resizable split: torrent table (left) + detail panel (right, conditional)
                h_resizable("main-split")
                    .with_state(&self.resizable_state)
                    .child(resizable_panel().child(DataTable::new(&self.table_state)))
                    .child(
                        resizable_panel()
                            .visible(self.detail_panel.is_some())
                            .size(px(400.))
                            .size_range(px(250.)..px(800.))
                            .child(
                                self.detail_panel
                                    .clone()
                                    .map(|p| p.into_any_element())
                                    .unwrap_or_else(|| div().into_any_element()),
                            ),
                    )
                    .into_any_element()
            })
            .children(self.magnet_dialog.as_ref().map(|dialog| {
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
                            .top(px(80.))
                            .left(px(80.))
                            .right(px(80.))
                            .bg(theme.background)
                            .rounded_md()
                            .border_1()
                            .border_color(theme.border)
                            .shadow_lg()
                            .p_4()
                            .overflow_hidden()
                            .child(dialog.clone())
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()),
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this: &mut MainPanel, _, _, cx| {
                            this.close_magnet_dialog(cx);
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

// ── Magnet Dialog ───────────────────────────────────────────────────────────

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

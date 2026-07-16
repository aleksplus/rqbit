use gpui::prelude::FluentBuilder as _;
use gpui::{WeakEntity, *};
use gpui_component::{
    ActiveTheme as _, StyledExt,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    h_flex,
    input::{Input, InputState},
    menu::PopupMenuItem,
    resizable::{ResizableState, resizable_panel, v_resizable},
    table::{Column, DataTable, TableDelegate, TableEvent, TableState},
    v_flex,
};
use librqbit::AddTorrentOptions;
use librqbit::api::ApiTorrentListOpts;
use librqbit::session_stats::snapshot::SessionStatsSnapshot;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use crate::state::State;
use crate::ui::add_torrent_dialog::{
    AddTorrentDialog, AddTorrentDialogEvent, AddTorrentEntry, AddTorrentSource,
};
use crate::ui::file_table::{FileRow, format_bytes};
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
    /// Torrent ID waiting for detail panel creation (deferred until window is available in render).
    pending_detail_id: Option<usize>,
    magnet_dialog: Option<Entity<MagnetDialog>>,
    delete_dialog: Option<Entity<DeleteDialog>>,
    add_dialog: Option<Entity<AddTorrentDialog>>,
    /// Torrent entries waiting for add-dialog creation (deferred until window is
    /// available in render, mirroring `pending_detail_id`).
    pending_add_entries: Option<Vec<AddTorrentEntry>>,
    /// Latest session-wide stats for the footer (download/upload speed, uptime).
    footer_stats: Option<SessionStatsSnapshot>,
    /// Keeps the periodic stats polling task alive for the lifetime of the panel.
    _stats_task: Option<Task<()>>,
    focus_handle: FocusHandle,
}

impl MainPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let state = cx.global::<State>().clone();

        let mut this = Self {
            state: Arc::new(state),
            // Placeholder; replaced just below once `this` exists.
            table_state: cx.new(|_cx| {
                TableState::new(
                    TorrentTableDelegate::new(WeakEntity::new_invalid()),
                    window,
                    _cx,
                )
                .row_selectable(true)
            }),
            resizable_state: cx.new(|_cx| ResizableState::default()),
            config_modal: None,
            detail_panel: None,
            pending_detail_id: None,
            magnet_dialog: None,
            delete_dialog: None,
            add_dialog: None,
            pending_add_entries: None,
            footer_stats: None,
            _stats_task: None,
            focus_handle: cx.focus_handle(),
        };

        // Now that `this` exists, build the real table state with a weak handle to it.
        let weak = cx.entity().downgrade();
        let table_state = cx.new(|cx| {
            TableState::new(TorrentTableDelegate::new(weak), window, cx).row_selectable(true)
        });
        this.table_state = table_state.clone();

        // Open/update detail panel when a row is selected in the table.
        cx.subscribe(
            &table_state,
            |this, table, event: &TableEvent, cx| match event {
                TableEvent::SelectRow(row_ix) => {
                    let torrent_id = table.read(cx).delegate().rows.get(*row_ix).map(|r| r.id);
                    if let Some(id) = torrent_id {
                        if let Some(panel) = &this.detail_panel {
                            // Update existing panel to show the newly selected torrent.
                            let _ = panel.update(cx, |panel, cx| {
                                panel.switch_torrent(id, cx);
                            });
                        } else {
                            // Mark that we need to create a panel; defer to render where window is available.
                            this.pending_detail_id = Some(id);
                            cx.notify();
                        }
                    }
                }
                _ => {}
            },
        )
        .detach();

        this.fetch_torrents(cx);
        this.start_stats_polling(cx);
        this
    }

    /// Periodically refresh session-wide stats so the footer shows live
    /// download/upload speed and uptime. Runs until the panel is dropped.
    fn start_stats_polling(&mut self, cx: &mut Context<Self>) {
        let api = self.state.api();
        let task = cx.spawn(async move |this, cx| {
            loop {
                let stats = api.api_session_stats();
                let _ = this.update(cx, |this, cx| {
                    this.footer_stats = Some(stats);
                    cx.notify();
                });
                cx.background_executor().timer(Duration::from_secs(1)).await;
            }
        });
        self._stats_task = Some(task);
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
                // Preserve selected torrent IDs across refresh by matching after update.
                let prev_selected_ids: HashSet<usize> = state
                    .delegate()
                    .selected_rows
                    .iter()
                    .filter_map(|&ix| state.delegate().rows.get(ix).map(|r| r.id))
                    .collect();
                state.delegate_mut().rows = rows;
                // Re-select rows whose torrent IDs match the previous selection.
                state.delegate_mut().selected_rows = state
                    .delegate()
                    .rows
                    .iter()
                    .enumerate()
                    .filter_map(|(ix, r)| {
                        if prev_selected_ids.contains(&r.id) {
                            Some(ix)
                        } else {
                            None
                        }
                    })
                    .collect();
                cx.notify();
            });

            let _ = this.update(cx, |_, cx| cx.notify());
        })
        .detach();
    }

    /// Get all selected torrent IDs (supports multi-row selection via shift-click).
    fn selected_torrent_ids(&self, cx: &mut Context<Self>) -> Vec<usize> {
        let table = self.table_state.read(cx);
        let delegate = table.delegate();
        let selected = &delegate.selected_rows;
        if selected.is_empty() {
            // Fall back to the table's single selected_row
            return table
                .selected_row()
                .and_then(|r| delegate.rows.get(r).map(|row| row.id))
                .into_iter()
                .collect();
        }
        selected
            .iter()
            .filter_map(|&ix| delegate.rows.get(ix).map(|row| row.id))
            .collect()
    }

    fn on_pause(&mut self, cx: &mut Context<Self>) {
        let ids = self.selected_torrent_ids(cx);
        if ids.is_empty() {
            return;
        }
        let api = self.state.api();
        cx.spawn(async move |_, _| {
            for id in &ids {
                let _ = api.api_torrent_action_pause((*id).into()).await;
            }
        })
        .detach();
        self.fetch_torrents(cx);
    }

    fn on_start(&mut self, cx: &mut Context<Self>) {
        let ids = self.selected_torrent_ids(cx);
        if ids.is_empty() {
            return;
        }
        let api = self.state.api();
        cx.spawn(async move |_, _| {
            for id in &ids {
                let _ = api.api_torrent_action_start((*id).into()).await;
            }
        })
        .detach();
        self.fetch_torrents(cx);
    }

    fn on_delete(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids = self.selected_torrent_ids(cx);
        if ids.is_empty() {
            return;
        }
        self.open_delete_dialog(ids, window, cx);
    }

    /// Open the delete confirmation dialog for the given torrent IDs.
    ///
    /// Uses a manually rendered overlay entity (same pattern as `MagnetDialog`)
    /// so it displays reliably without depending on a global dialog layer.
    fn open_delete_dialog(
        &mut self,
        ids: Vec<usize>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if ids.is_empty() {
            return;
        }
        let api = self.state.api();
        // Resolve names for display (synchronous API call).
        let names: Vec<String> = ids
            .iter()
            .filter_map(|id| {
                api.api_torrent_details((*id).into())
                    .ok()
                    .and_then(|d| d.name)
                    .or_else(|| Some(format!("Torrent #{}", id)))
            })
            .collect();

        let dialog = cx.new(|cx| DeleteDialog::new(ids.clone(), names, _window, cx));
        cx.subscribe(
            &dialog,
            |this, _entity, event: &DeleteDialogEvent, cx| match event {
                DeleteDialogEvent::Cancelled => this.close_delete_dialog(cx),
                DeleteDialogEvent::Confirmed(ids, delete_files) => {
                    this.close_delete_dialog(cx);
                    this.confirm_delete(ids.clone(), *delete_files, cx);
                }
            },
        )
        .detach();
        self.delete_dialog = Some(dialog);
        cx.notify();
    }

    fn close_delete_dialog(&mut self, cx: &mut Context<Self>) {
        self.delete_dialog = None;
        cx.notify();
    }

    /// Actually perform the delete after the dialog is confirmed.
    ///
    /// When `delete_files` is true the downloaded data is removed from disk
    /// (`api_torrent_action_delete`); otherwise only the torrent entry is removed
    /// and the files are kept (`api_torrent_action_forget`).
    fn confirm_delete(&mut self, ids: Vec<usize>, delete_files: bool, cx: &mut Context<Self>) {
        let api = self.state.api();
        cx.spawn(async move |_, _| {
            for id in &ids {
                if delete_files {
                    let _ = api.api_torrent_action_delete((*id).into()).await;
                } else {
                    let _ = api.api_torrent_action_forget((*id).into()).await;
                }
            }
        })
        .detach();
        self.fetch_torrents(cx);
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
                    let mut entries: Vec<AddTorrentEntry> = Vec::new();
                    for path in paths {
                        match std::fs::read(&path) {
                            Ok(bytes) => {
                                let source = AddTorrentSource::Bytes(bytes);
                                match build_entry(&api, source).await {
                                    Ok(entry) => entries.push(entry),
                                    Err(e) => {
                                        eprintln!("Error reading torrent {:?}: {:?}", path, e)
                                    }
                                }
                            }
                            Err(e) => {
                                eprintln!("Error reading file {:?}: {:?}", path, e);
                            }
                        }
                    }
                    if !entries.is_empty() {
                        let _ = this.update(cx, |this, cx| {
                            this.open_add_dialog(entries, cx);
                        });
                    }
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
            let source = AddTorrentSource::Url(magnet);
            match build_entry(&api, source).await {
                Ok(entry) => {
                    let _ = this.update(cx, |this, cx| {
                        this.open_add_dialog(vec![entry], cx);
                    });
                }
                Err(e) => eprintln!("Error preparing magnet: {:?}", e),
            }
        })
        .detach();
    }

    fn close_magnet_dialog(&mut self, cx: &mut Context<Self>) {
        self.magnet_dialog = None;
        cx.notify();
    }

    /// Defer creation of the file-selection dialog until `render`, where a
    /// `Window` is available (mirrors `pending_detail_id`).
    fn open_add_dialog(&mut self, entries: Vec<AddTorrentEntry>, cx: &mut Context<Self>) {
        self.pending_add_entries = Some(entries);
        cx.notify();
    }

    /// Actually create the add dialog from pending entries (called from render).
    fn create_add_dialog(
        &mut self,
        entries: Vec<AddTorrentEntry>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dialog = cx.new(|cx| AddTorrentDialog::new(entries, window, cx));
        cx.subscribe(
            &dialog,
            |this, _entity, event: &AddTorrentDialogEvent, cx| match event {
                AddTorrentDialogEvent::Cancelled => this.close_add_dialog(cx),
                AddTorrentDialogEvent::AddOne(source, selection) => {
                    this.confirm_add(source.clone(), selection.clone(), cx);
                }
                AddTorrentDialogEvent::Finished => this.close_add_dialog(cx),
            },
        )
        .detach();
        self.add_dialog = Some(dialog);
        cx.notify();
    }

    fn close_add_dialog(&mut self, cx: &mut Context<Self>) {
        self.add_dialog = None;
        cx.notify();
    }

    /// Actually add a torrent, applying the selected file indices (if any).
    fn confirm_add(
        &mut self,
        source: AddTorrentSource,
        selection: Option<Vec<usize>>,
        cx: &mut Context<Self>,
    ) {
        let api = self.state.api();
        cx.spawn(async move |this, cx| {
            let opts = selection.map(|only_files| AddTorrentOptions {
                only_files: Some(only_files),
                ..Default::default()
            });
            if let Err(e) = api.api_add_torrent(source.to_add(), opts).await {
                eprintln!("Error adding torrent: {:?}", e);
            }
            let _ = this.update(cx, |this, cx| this.fetch_torrents(cx));
        })
        .detach();
    }

    fn create_detail_panel(
        &mut self,
        torrent_id: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let state = self.state.clone();
        let panel = cx.new(|cx| TorrentDetailPanel::new(torrent_id, window, cx, state));
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

    fn close_details(&mut self, cx: &mut Context<Self>) {
        // Deselect the table row so the detail panel stays hidden.
        let _ = self.table_state.update(cx, |state, cx| {
            state.delegate_mut().selected_rows.clear();
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
    /// Multi-selection state: set of selected row indices.
    selected_rows: HashSet<usize>,
    /// The anchor row for shift-click range selection.
    anchor_row: Option<usize>,
    /// Weak handle to the parent [`MainPanel`] so the context menu can open dialogs.
    main_panel: WeakEntity<MainPanel>,
}

impl TorrentTableDelegate {
    fn new(main_panel: WeakEntity<MainPanel>) -> Self {
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
            selected_rows: HashSet::new(),
            anchor_row: None,
            main_panel,
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

    fn render_tr(
        &mut self,
        row_ix: usize,
        _window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> Stateful<Div> {
        let is_selected = self.selected_rows.contains(&row_ix);
        div()
            .id(("torrent-row", row_ix))
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
        menu: gpui_component::menu::PopupMenu,
        _window: &mut Window,
        _cx: &mut Context<TableState<Self>>,
    ) -> gpui_component::menu::PopupMenu {
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

        menu.item(
            PopupMenuItem::new(format!("Pause: {torrent_name}")).on_click(
                move |_, _, cx: &mut App| {
                    let api = cx.global::<State>().api();
                    cx.spawn(async move |_| {
                        let _ = api.api_torrent_action_pause(torrent_id.into()).await;
                    })
                    .detach();
                },
            ),
        )
        .item(
            PopupMenuItem::new(format!("Start: {torrent_name}")).on_click(
                move |_, _, cx: &mut App| {
                    let api = cx.global::<State>().api();
                    cx.spawn(async move |_| {
                        let _ = api.api_torrent_action_start(torrent_id.into()).await;
                    })
                    .detach();
                },
            ),
        )
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

/// Format a duration in seconds as a compact human-readable uptime string.
fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    if days > 0 {
        format!("{days}d {hours}h {minutes}m")
    } else if hours > 0 {
        format!("{hours}h {minutes}m {secs}s")
    } else if minutes > 0 {
        format!("{minutes}m {secs}s")
    } else {
        format!("{secs}s")
    }
}

impl Render for MainPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Create pending detail panel now that window is available.
        let pending_id = self.pending_detail_id.take();
        if let Some(id) = pending_id {
            self.create_detail_panel(id, _window, cx);
        }

        // Create pending add dialog now that window is available.
        let pending_add = self.pending_add_entries.take();
        if let Some(entries) = pending_add {
            self.create_add_dialog(entries, _window, cx);
        }

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
                        Button::new("delete").label("Delete").on_click(
                            cx.listener(|this, _, window, cx| this.on_delete(window, cx)),
                        ),
                    )
                    .child(
                        Button::new("settings").label("Settings").on_click(
                            cx.listener(|this, _, window, cx| this.on_settings(window, cx)),
                        ),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .child(if let Some(settings) = &self.config_modal {
                        // Settings page replaces the main content
                        div().size_full().child(settings.clone()).into_any_element()
                    } else {
                        // Resizable split: torrent table (left) + detail panel (right, conditional)
                        v_resizable("main-split")
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
                    }),
            )
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
            .children(self.delete_dialog.as_ref().map(|dialog| {
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
                            this.close_delete_dialog(cx);
                        }),
                    )
            }))
            .children(self.add_dialog.as_ref().map(|dialog| {
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
                            .max_h(px(600.))
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
                            this.close_add_dialog(cx);
                        }),
                    )
            }))
            .child(self.render_footer(cx))
    }
}

impl MainPanel {
    /// Render the footer showing session-wide download/upload speed and uptime.
    fn render_footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();

        let (down_speed, up_speed, fetched, uploaded, uptime) = match &self.footer_stats {
            Some(stats) => (
                stats.download_speed.to_string(),
                stats.upload_speed.to_string(),
                format_bytes(stats.counters.fetched_bytes),
                format_bytes(stats.counters.uploaded_bytes),
                format_uptime(stats.uptime_seconds),
            ),
            None => (
                "—".to_string(),
                "—".to_string(),
                "—".to_string(),
                "—".to_string(),
                "—".to_string(),
            ),
        };

        h_flex()
            .flex_shrink_0()
            .justify_between()
            .gap_4()
            .px_3()
            .py_1()
            .bg(theme.background)
            .border_t_1()
            .border_color(theme.border)
            .text_sm()
            .text_color(theme.muted_foreground)
            .child(
                h_flex()
                    .gap_1()
                    .child(div().child("↓ ").font_medium())
                    .child(div().child(down_speed))
                    .child(
                        div()
                            .child(format!("({fetched})"))
                            .text_color(theme.muted_foreground),
                    ),
            )
            .child(
                h_flex()
                    .gap_1()
                    .child(div().child("↑ ").font_medium())
                    .child(div().child(up_speed))
                    .child(
                        div()
                            .child(format!("({uploaded})"))
                            .text_color(theme.muted_foreground),
                    ),
            )
            .child(
                h_flex()
                    .gap_1()
                    .child(div().child("up ").font_medium())
                    .child(div().child(uptime)),
            )
    }
}

/// Build an [`AddTorrentEntry`] by resolving the torrent's file list via
/// `list_only`. If the file list can't be resolved (e.g. a magnet that can't
/// be fetched right now), the entry is created with no files so it will be
/// added with all files selected.
async fn build_entry(
    api: &librqbit::Api,
    source: AddTorrentSource,
) -> anyhow::Result<AddTorrentEntry> {
    let list_opts = AddTorrentOptions {
        list_only: true,
        ..Default::default()
    };
    let response = api
        .api_add_torrent(source.to_add(), Some(list_opts))
        .await?;
    let name = response
        .details
        .name
        .clone()
        .unwrap_or_else(|| response.details.info_hash.clone());
    let files: Vec<FileRow> = response
        .details
        .files
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(idx, f)| FileRow {
            file_index: idx,
            name: f.name,
            length: f.length,
            included: true,
        })
        .collect();
    Ok(AddTorrentEntry {
        source,
        name,
        files,
    })
}

impl Focusable for MainPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

// ── Delete Dialog ──────────────────────────────────────────────────────────

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
            delete_files: false,
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

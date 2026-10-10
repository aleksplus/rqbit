use gpui::prelude::FluentBuilder as _;
use gpui::{WeakEntity, *};
use gpui_kit::component::separator::Separator;
use gpui_kit::component::status_bar::StatusBar;
use gpui_kit::component::{ActiveTheme, Sizable};
use gpui_kit::component::{
    IconName, Size, StyledExt, TitleBar,
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputEvent, InputState},
    resizable::{ResizableState, resizable_panel, v_resizable},
    searchable_list::SearchableVec,
    select::{Select, SelectEvent, SelectState},
    table::{DataTable, TableEvent, TableState},
    v_flex,
};
use gpui_kit::component::{Theme, ThemeMode};
use librqbit::AddTorrentOptions;
use librqbit::api::{ApiTorrentListOpts, EmptyJsonResponse};
use librqbit::session_stats::snapshot::SessionStatsSnapshot;
use std::collections::HashSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use crate::hooks::{HookEvent, TorrentAddedPayload, TorrentDeletedPayload, TorrentInfo, envelope};
use crate::state::State;
use crate::ui::add_torrent_dialog::{
    AddTorrentDialog, AddTorrentDialogEvent, AddTorrentEntry, AddTorrentSource,
};

use crate::ui::dialogs::{DeleteDialog, DeleteDialogEvent, MagnetDialog, MagnetDialogEvent};
use crate::ui::settings_page::{SettingsPage, SettingsPageEvent};
use crate::ui::torrent_detail_panel::{TorrentDetailPanel, TorrentDetailPanelEvent};
use crate::ui::torrent_table::{TorrentAction, TorrentRow, TorrentTableDelegate};
use crate::ui::utils::{build_entry, format_bytes, format_uptime, render_modal_overlay};

/// Future returned by a per-torrent API action dispatched by
/// [`MainPanel::run_action_for_ids`]. All `api_torrent_action_*` methods share
/// the same output type.
type TorrentApiActionFuture =
    Pin<Box<dyn Future<Output = librqbit::api::Result<EmptyJsonResponse>> + Send>>;

/// The modal dialog currently open, if any. At most one dialog can be open at
/// a time; opening a new one replaces the previous.
enum ActiveDialog {
    Magnet(Entity<MagnetDialog>),
    Delete(Entity<DeleteDialog>),
    Add(Entity<AddTorrentDialog>),
}

/// Run configured `file_selection` hooks against an entry's resolved file
/// list and record which ones changed the selection (shown as a hint in the
/// add dialog). A no-op when no such hooks are enabled.
async fn apply_file_selection_hooks(state: &Arc<State>, entry: &mut AddTorrentEntry) {
    let applied = state.hooks().apply_file_selection(entry).await;
    if !applied.is_empty() {
        entry.hook_note = Some(applied.join(", "));
    }
}

/// Main panel that displays the list of torrents.
pub struct MainPanel {
    state: Arc<State>,
    table_state: Entity<TableState<TorrentTableDelegate>>,
    resizable_state: Entity<ResizableState>,
    settings_sidebar: Option<Entity<SettingsPage>>,
    detail_panel: Option<Entity<TorrentDetailPanel>>,
    /// Torrent ID waiting for detail panel creation (deferred until window is available in render).
    pending_detail_id: Option<usize>,
    dialog: Option<ActiveDialog>,
    /// Torrent entries waiting for add-dialog creation (deferred until window is
    /// available in render, mirroring `pending_detail_id`).
    pending_add_entries: Option<Vec<AddTorrentEntry>>,
    /// Latest session-wide stats for the footer (download/upload speed, uptime).
    footer_stats: Option<SessionStatsSnapshot>,
    /// Search box for filtering torrents by name.
    search_input: Entity<InputState>,
    /// Status filter dropdown (empty string = "All").
    status_filter: Entity<SelectState<SearchableVec<String>>>,
    /// Keeps the periodic stats polling task alive for the lifetime of the panel.
    _stats_task: Option<Task<()>>,
    /// Whether the window is currently active (focused). When the window is
    /// minimized or hidden, this is false and we throttle table refreshes.
    window_active: bool,
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
            settings_sidebar: None,
            detail_panel: None,
            pending_detail_id: None,
            dialog: None,
            pending_add_entries: None,
            footer_stats: None,
            search_input: cx
                .new(|_cx| InputState::new(window, _cx).placeholder("Search torrents…")),
            status_filter: cx.new(|_cx| {
                SelectState::new(
                    SearchableVec::new(vec![
                        "".to_string(),
                        "initializing".to_string(),
                        "live".to_string(),
                        "paused".to_string(),
                        "error".to_string(),
                    ]),
                    None,
                    window,
                    _cx,
                )
            }),
            _stats_task: None,
            window_active: true,
            focus_handle: cx.focus_handle(),
        };

        // Now that `this` exists, build the real table state with a weak handle to it.
        let weak = cx.entity().downgrade();
        let table_state = cx.new(|cx| {
            TableState::new(TorrentTableDelegate::new(weak), window, cx).row_selectable(true)
        });
        this.table_state = table_state.clone();

        // Open/update detail panel when a row is selected in the table.
        cx.subscribe(&table_state, |this, table, event: &TableEvent, cx| {
            if let TableEvent::SelectRow(row_ix) = event {
                let torrent_id = table.read(cx).delegate().rows.get(*row_ix).map(|r| r.id);
                if let Some(id) = torrent_id {
                    if let Some(panel) = &this.detail_panel {
                        // Update existing panel to show the newly selected torrent.
                        panel.update(cx, |panel, cx| {
                            panel.switch_torrent(id, cx);
                        });
                    } else {
                        // Mark that we need to create a panel; defer to render where window is available.
                        this.pending_detail_id = Some(id);
                        cx.notify();
                    }
                }
            }
        })
        .detach();

        // Track window activation so we can throttle table refreshes when the
        // window is minimized or hidden (inactive).
        cx.observe_window_activation(window, |this, window, cx| {
            let was_active = this.window_active;
            this.window_active = window.is_window_active();

            // When window becomes active after being inactive, refresh immediately
            // instead of waiting for the next polling cycle.
            if this.window_active && !was_active {
                this.fetch_torrents(cx);
                if let Some(panel) = &this.detail_panel {
                    let selected_ids = this.selected_torrent_ids(cx);
                    if let Some(torrent_id) = selected_ids.first().copied() {
                        panel.update(cx, |panel, cx| {
                            panel.switch_torrent(torrent_id, cx);
                        });
                    }
                }
            }

            cx.notify();
        })
        .detach();

        // Re-apply the search/filter whenever the search box text changes.
        let search_input = this.search_input.clone();
        cx.subscribe(&search_input, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.apply_search_and_filter(cx);
            }
        })
        .detach();

        // Re-apply the search/filter whenever the status dropdown changes.
        let status_filter = this.status_filter.clone();
        cx.subscribe(
            &status_filter,
            |this, _, _event: &SelectEvent<SearchableVec<String>>, cx| {
                this.apply_search_and_filter(cx);
            },
        )
        .detach();

        this.fetch_torrents(cx);
        this.start_stats_polling(cx);
        this
    }

    /// Periodically refresh session-wide stats (footer) and the torrent table.
    /// When the window is inactive (minimized/hidden) the table refresh is
    /// throttled to a much longer interval to reduce unnecessary work.
    fn start_stats_polling(&mut self, cx: &mut Context<Self>) {
        let api = self.state.api();
        let task = cx.spawn(async move |this, cx| {
            loop {
                let stats = api.api_session_stats();
                let _ = this.update(cx, |this, cx| {
                    this.footer_stats = Some(stats);
                    cx.notify();
                });

                // Refresh the torrent table. When the window is inactive we
                // skip the per-second refresh and only poll occasionally.
                let active = this
                    .update(cx, |this, _cx| this.window_active)
                    .unwrap_or(true);
                if active {
                    let _ = this.update(cx, |this, cx| this.fetch_torrents(cx));
                    cx.background_executor().timer(Duration::from_secs(1)).await;
                } else {
                    cx.background_executor()
                        .timer(Duration::from_secs(30))
                        .await;
                }
            }
        });
        self._stats_task = Some(task);
    }

    /// Push the current search text and status filter into the table delegate,
    /// which re-derives the visible rows. Safe to call on every keystroke /
    /// dropdown change; the full `all_rows` set is preserved so live status
    /// updates are not lost.
    fn apply_search_and_filter(&mut self, cx: &mut Context<Self>) {
        let search = self.search_input.read(cx).value().to_string();
        let status = self
            .status_filter
            .read(cx)
            .selected_value()
            .map(|s| s.to_string())
            .unwrap_or_default();
        self.table_state.update(cx, |state, cx| {
            state.delegate_mut().set_filter(search, status);
            cx.notify();
        });
        cx.notify();
    }

    fn fetch_torrents(&mut self, cx: &mut Context<Self>) {
        let api = self.state.api();
        let table_state = self.table_state.clone();
        cx.spawn(async move |this, cx| {
            let response = api.api_torrent_list_ext(ApiTorrentListOpts { with_stats: true });
            let rows: Vec<TorrentRow> = response
                .torrents
                .iter()
                .map(TorrentRow::from_listed)
                .collect();

            table_state.update(cx, |state, cx| {
                // Preserve selected torrent IDs across refresh by matching after update.
                let prev_selected_ids: HashSet<usize> = state
                    .delegate()
                    .selected_rows
                    .iter()
                    .filter_map(|&ix| state.delegate().rows.get(ix).map(|r| r.id))
                    .collect();
                state.delegate_mut().all_rows = rows;
                // Derive the visible rows from the full set using the filter
                // stored in the delegate, then re-apply the active sort.
                state.delegate_mut().apply_filter();
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
        self.run_action_on_selected(cx, TorrentAction::Pause);
    }

    fn on_start(&mut self, cx: &mut Context<Self>) {
        self.run_action_on_selected(cx, TorrentAction::Start);
    }

    /// Apply a pause/start action to all currently selected torrents, then
    /// clear the selection and refresh the table.
    fn run_action_on_selected(&mut self, cx: &mut Context<Self>, action: TorrentAction) {
        let ids = self.selected_torrent_ids(cx);
        if ids.is_empty() {
            return;
        }
        let api = self.state.api();
        match action {
            TorrentAction::Pause => self.run_action_for_ids(
                ids,
                move |id| {
                    let api = api.clone();
                    Box::pin(async move { api.api_torrent_action_pause(id.into()).await })
                },
                cx,
            ),
            TorrentAction::Start => self.run_action_for_ids(
                ids,
                move |id| {
                    let api = api.clone();
                    Box::pin(async move { api.api_torrent_action_start(id.into()).await })
                },
                cx,
            ),
        }
    }

    /// Run `action` for each torrent ID sequentially, then clear the table
    /// selection and refresh. Shared by the pause/start and delete/forget
    /// flows. `action` clones the `Api` handle per call and returns a boxed
    /// future so each variant can dispatch its own API method.
    fn run_action_for_ids<F>(&mut self, ids: Vec<usize>, action: F, cx: &mut Context<Self>)
    where
        F: Fn(usize) -> TorrentApiActionFuture + Send + 'static,
    {
        let table_state = self.table_state.clone();
        cx.spawn(async move |_, cx| {
            for id in ids {
                if let Err(e) = action(id).await {
                    eprintln!("Torrent action failed: {:?}", e);
                }
            }
            table_state.update(cx, |state, cx| {
                state.delegate_mut().selected_rows.clear();
                cx.notify();
            });
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
    /// Public so the torrent table's context menu can open it.
    pub fn open_delete_dialog(
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
                DeleteDialogEvent::Cancelled => this.close_dialog(cx),
                DeleteDialogEvent::Confirmed(ids, delete_files) => {
                    this.close_dialog(cx);
                    this.confirm_delete(ids.clone(), *delete_files, cx);
                }
            },
        )
        .detach();
        self.dialog = Some(ActiveDialog::Delete(dialog));
        cx.notify();
    }

    /// Close the currently open modal dialog, if any.
    fn close_dialog(&mut self, cx: &mut Context<Self>) {
        self.dialog = None;
        cx.notify();
    }

    /// Actually perform the delete after the dialog is confirmed.
    ///
    /// When `delete_files` is true the downloaded data is removed from disk
    /// (`api_torrent_action_delete`); otherwise only the torrent entry is removed
    /// and the files are kept (`api_torrent_action_forget`). A successful
    /// delete/forget fires the `torrent_deleted` hook.
    fn confirm_delete(&mut self, ids: Vec<usize>, delete_files: bool, cx: &mut Context<Self>) {
        let api = self.state.api();
        let hooks = self.state.hooks();
        self.run_action_for_ids(
            ids,
            move |id| {
                let api = api.clone();
                let hooks = hooks.clone();
                Box::pin(async move {
                    // Resolve details first — the handle is gone after deletion.
                    let details = api.api_torrent_details(id.into()).ok();
                    let result = if delete_files {
                        api.api_torrent_action_delete(id.into()).await
                    } else {
                        api.api_torrent_action_forget(id.into()).await
                    };
                    if result.is_ok()
                        && let Some(d) = details
                    {
                        hooks.forget(id);
                        hooks.dispatch(
                            HookEvent::TorrentDeleted,
                            envelope(
                                HookEvent::TorrentDeleted,
                                TorrentDeletedPayload {
                                    torrent: TorrentInfo {
                                        id: Some(id),
                                        name: d.name,
                                        info_hash: d.info_hash,
                                        output_folder: d.output_folder,
                                    },
                                    delete_files,
                                },
                            ),
                        );
                    }
                    result
                })
            },
            cx,
        );
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

        let state = self.state.clone();
        cx.spawn(async move |this, cx| {
            let api = state.api();
            match receiver.await {
                Ok(Ok(Some(paths))) => {
                    let mut entries: Vec<AddTorrentEntry> = Vec::new();
                    for path in paths {
                        match std::fs::read(&path) {
                            Ok(bytes) => {
                                let source = AddTorrentSource::Bytes(bytes);
                                match build_entry(&api, source).await {
                                    Ok(mut entry) => {
                                        apply_file_selection_hooks(&state, &mut entry).await;
                                        entries.push(entry);
                                    }
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
                MagnetDialogEvent::Cancelled => this.close_dialog(cx),
                MagnetDialogEvent::Submitted(magnet) => {
                    this.close_dialog(cx);
                    this.add_magnet(magnet.clone(), cx);
                }
            },
        )
        .detach();
        self.dialog = Some(ActiveDialog::Magnet(dialog));
        cx.notify();
    }

    fn add_magnet(&mut self, magnet: String, cx: &mut Context<Self>) {
        let state = self.state.clone();
        cx.spawn(async move |this, cx| {
            let api = state.api();
            let source = AddTorrentSource::Url(magnet);
            match build_entry(&api, source).await {
                Ok(mut entry) => {
                    apply_file_selection_hooks(&state, &mut entry).await;
                    let _ = this.update(cx, |this, cx| {
                        this.open_add_dialog(vec![entry], cx);
                    });
                }
                Err(e) => eprintln!("Error preparing magnet: {:?}", e),
            }
        })
        .detach();
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
                AddTorrentDialogEvent::Cancelled => this.close_dialog(cx),
                AddTorrentDialogEvent::AddOne(source, selection, overwrite) => {
                    this.confirm_add(source.clone(), selection.clone(), *overwrite, cx);
                }
                AddTorrentDialogEvent::Finished => this.close_dialog(cx),
            },
        )
        .detach();
        self.dialog = Some(ActiveDialog::Add(dialog));
        cx.notify();
    }

    /// Actually add a torrent, applying the selected file indices (if any),
    /// then fire the `torrent_added` hook (skipped when the torrent was
    /// already managed by the session — the API response does not
    /// distinguish that case, so the hook watcher's known-ID set decides).
    fn confirm_add(
        &mut self,
        source: AddTorrentSource,
        selection: Option<Vec<usize>>,
        overwrite: bool,
        cx: &mut Context<Self>,
    ) {
        let api = self.state.api();
        let hooks = self.state.hooks();
        cx.spawn(async move |this, cx| {
            let opts = Some(AddTorrentOptions {
                only_files: selection.clone(),
                overwrite,
                ..Default::default()
            });
            match api.api_add_torrent(source.to_add(), opts).await {
                Ok(response) => {
                    if let Some(id) = response.id
                        && !hooks.is_known(id)
                    {
                        hooks.prime_added(id);
                        let files = response.details.files.as_deref().unwrap_or(&[]);
                        hooks.dispatch(
                            HookEvent::TorrentAdded,
                            envelope(
                                HookEvent::TorrentAdded,
                                TorrentAddedPayload {
                                    torrent: TorrentInfo {
                                        id: Some(id),
                                        name: response.details.name,
                                        info_hash: response.details.info_hash,
                                        output_folder: response.output_folder,
                                    },
                                    files_total: files.len(),
                                    files_selected: files.iter().filter(|f| f.included).count(),
                                    selected_indices: selection,
                                },
                            ),
                        );
                    }
                }
                Err(e) => eprintln!("Error adding torrent: {:?}", e),
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
        self.table_state.update(cx, |state, cx| {
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
        self.settings_sidebar = Some(page);
        cx.notify();
    }

    fn close_settings(&mut self, cx: &mut Context<Self>) {
        self.settings_sidebar = None;
        cx.notify();
    }

    /// Toggle between light and dark theme, persist the choice to the config
    /// file, and apply it to the active window.
    fn toggle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let is_dark = cx.theme().is_dark();
        let new_mode = if is_dark {
            ThemeMode::Light
        } else {
            ThemeMode::Dark
        };

        // Apply the new theme to the window immediately.
        Theme::change(new_mode, Some(window), cx);

        // Persist the choice to the config file.
        let mut config = self.state.config();
        config.theme = new_mode.name().to_string();
        if let Err(e) = crate::config::write_config(&self.state.config_filename, &config) {
            eprintln!("Error writing config for theme: {:?}", e);
        } else {
            // Update the in-memory shared config so other views see the change.
            self.state.shared().write().set_config(config);
        }

        cx.notify();
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

        v_flex()
            .size_full()
            .gap_0()
            .child(self.render_title_bar(cx))
            .when(self.settings_sidebar.is_none(), |this| {
                this.child(self.render_toolbar(cx))
            })
            .child(self.render_content(cx))
            .children(self.render_overlays(cx))
            .child(self.render_footer(cx))
    }
}

impl MainPanel {
    /// Render the application title bar: app name/version plus the global
    /// action buttons (add torrent/magnet, settings, theme toggle). Draggable;
    /// handles macOS traffic-light padding.
    fn render_title_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        TitleBar::new()
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().text_base().font_semibold().child("Rqbit Desktop"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(format!("v{}", librqbit::version())),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        Button::new("add-torrent")
                            .label("Add Torrent…")
                            .small()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.on_add_torrent_files(window, cx)
                            })),
                    )
                    .child(
                        Button::new("add-magnet")
                            .label("Add Magnet…")
                            .small()
                            .on_click(
                                cx.listener(|this, _, window, cx| this.on_add_magnet(window, cx)),
                            ),
                    )
                    .child(
                        Button::new("settings")
                            .icon(IconName::Settings)
                            .small()
                            .ghost()
                            .on_click(
                                cx.listener(|this, _, window, cx| this.on_settings(window, cx)),
                            ),
                    )
                    .child(
                        Button::new("theme")
                            .icon(if cx.theme().is_dark() {
                                IconName::Sun
                            } else {
                                IconName::Moon
                            })
                            .small()
                            .ghost()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.toggle_theme(window, cx);
                            })),
                    ),
            )
            .pr(px(10.))
    }

    /// Render the toolbar: selection actions on the left, search box and
    /// status filter on the right. Extra left padding for macOS traffic lights.
    fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        h_flex()
            .gap_2()
            .bg(theme.background)
            .border_b_1()
            .border_color(theme.border)
            .pl(px(10.))
            .pt(px(5.))
            .pr(px(10.))
            .pb(px(5.))
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Button::new("start")
                            .label("Start")
                            .icon(IconName::Play)
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| this.on_start(cx))),
                    )
                    .child(
                        Button::new("pause")
                            .label("Pause")
                            .icon(IconName::Pause)
                            .small()
                            .on_click(cx.listener(|this, _, _, cx| this.on_pause(cx))),
                    )
                    .child(
                        Button::new("delete")
                            .label("Delete")
                            .icon(IconName::CircleX)
                            .small()
                            .on_click(
                                cx.listener(|this, _, window, cx| this.on_delete(window, cx)),
                            ),
                    )
                    .child(
                        div().pl_4().child(
                            Button::new("refresh")
                                .tooltip("Refresh")
                                .icon(IconName::LoaderCircle)
                                .small()
                                .on_click(cx.listener(|this, _, _, cx| this.on_refresh(cx))),
                        ),
                    ),
            )
            .child(h_flex().flex_grow_1().size_full())
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        Select::new(&self.status_filter)
                            .with_size(Size::Small)
                            .placeholder("All statuses")
                            .w(px(100.))
                            .size_full(),
                    )
                    .child(
                        Input::new(&self.search_input)
                            .with_size(Size::Small)
                            .cleanable(true)
                            .min_w(px(200.))
                            .w(px(200.)),
                    ),
            )
    }

    /// Render the main content area: the settings page when open, otherwise
    /// the resizable split of torrent table (left) + detail panel (right,
    /// conditional).
    fn render_content(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex_1()
            .min_h_0()
            .child(if let Some(settings) = &self.settings_sidebar {
                // Settings page replaces the main content.
                div().size_full().child(settings.clone()).into_any_element()
            } else {
                v_resizable("main-split")
                    .with_state(&self.resizable_state)
                    .child(
                        resizable_panel().child(
                            div()
                                .size_full()
                                .overflow_hidden()
                                .border_1()
                                .border_color(cx.theme().border)
                                .child(DataTable::new(&self.table_state).bordered(false)),
                        ),
                    )
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
    }

    /// Render the modal overlay for the open dialog (magnet, delete, or add),
    /// if any.
    fn render_overlays(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut overlays = Vec::new();
        match &self.dialog {
            Some(ActiveDialog::Magnet(dialog)) => overlays.push(
                render_modal_overlay(dialog, MainPanel::close_dialog, None, cx).into_any_element(),
            ),
            Some(ActiveDialog::Delete(dialog)) => overlays.push(
                render_modal_overlay(dialog, MainPanel::close_dialog, None, cx).into_any_element(),
            ),
            Some(ActiveDialog::Add(dialog)) => overlays.push(
                render_modal_overlay(dialog, MainPanel::close_dialog, Some(px(600.)), cx)
                    .into_any_element(),
            ),
            None => {}
        }
        overlays
    }

    /// Render the footer showing session-wide download/upload speed and uptime.
    ///
    /// Uses the gpui-component [`StatusBar`] with three left-pinned groups
    /// (download, upload, uptime) separated by vertical dividers.
    fn render_footer(&self, cx: &mut Context<Self>) -> impl IntoElement {
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

        StatusBar::new()
            // ↓ Download speed (and total fetched)
            .left(
                h_flex()
                    .items_center()
                    .gap_1()
                    .child(div().font_medium().child("↓"))
                    .child(div().child(down_speed))
                    .child(
                        div()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("({fetched})")),
                    ),
            )
            .left(Separator::vertical())
            // ↑ Upload speed (and total uploaded)
            .left(
                h_flex()
                    .items_center()
                    .gap_1()
                    .child(div().font_medium().child("↑"))
                    .child(div().child(up_speed))
                    .child(
                        div()
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("({uploaded})")),
                    ),
            )
            .left(Separator::vertical())
            // Uptime
            .right(
                h_flex()
                    .items_center()
                    .gap_1()
                    .child(div().font_medium().child("up"))
                    .child(div().child(uptime)),
            )
    }
}

impl Focusable for MainPanel {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

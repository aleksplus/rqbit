//! Shared formatting helpers for the desktop UI.

use std::path::PathBuf;

use gpui::{
    App, Context, Entity, InteractiveElement, IntoElement, MouseButton, ParentElement, Pixels,
    Render, Styled, div, px,
};
use gpui_component::{ActiveTheme, menu::PopupMenuItem, v_flex};
use librqbit::AddTorrentOptions;

use crate::State;
use crate::ui::{
    add_torrent_dialog::{AddTorrentEntry, AddTorrentSource},
    file_table::FileRow,
    main_panel::{MainPanel, TorrentAction},
};

/// Binary unit multipliers (powers of 1024), computed once and reused.
const KIB: f64 = 1024.0;
const MIB: f64 = KIB * 1024.0;
const GIB: f64 = MIB * 1024.0;
const TIB: f64 = GIB * 1024.0;

/// Format a byte count into a human-readable binary unit string.
///
/// `suffix` is appended after the unit (e.g. `""` for sizes, `"/s"` for
/// speeds), `decimals` controls the number of fractional digits, and `labels`
/// supplies the four unit prefixes for the TiB/GiB/MiB/KiB tiers (top to
/// bottom). Values below `KIB` are rendered as whole bytes.
fn format_binary(bytes: f64, suffix: &str, decimals: usize, labels: [&str; 4]) -> String {
    let (div, unit) = if bytes >= TIB {
        (TIB, labels[0])
    } else if bytes >= GIB {
        (GIB, labels[1])
    } else if bytes >= MIB {
        (MIB, labels[2])
    } else if bytes >= KIB {
        (KIB, labels[3])
    } else {
        return format!("{:.0} B{}", bytes, suffix);
    };
    format!("{0:.1$} {2}B{3}", bytes / div, decimals, unit, suffix)
}

/// Format a byte count in human-readable form (binary units).
pub fn format_bytes(bytes: u64) -> String {
    format_binary(bytes as f64, "", 2, ["Ti", "Gi", "Mi", "Ki"])
}

/// Format a speed (given in Mbps) in human-readable form.
pub fn format_speed(mbps: f64) -> String {
    format_binary(mbps * MIB, "/s", 1, ["G", "G", "M", "K"])
}

/// Format a duration in seconds as a compact human-readable uptime string.
pub fn format_uptime(seconds: u64) -> String {
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

/// Build a context-menu item that runs a pause/start action on a torrent.
pub fn torrent_action_menu_item(
    label: String,
    torrent_id: usize,
    action: TorrentAction,
) -> PopupMenuItem {
    PopupMenuItem::new(label).on_click(move |_, _, cx: &mut App| {
        let api = cx.global::<State>().api();
        cx.spawn(async move |_| match action {
            TorrentAction::Pause => {
                let _ = api.api_torrent_action_pause(torrent_id.into()).await;
            }
            TorrentAction::Start => {
                let _ = api.api_torrent_action_start(torrent_id.into()).await;
            }
        })
        .detach();
    })
}

/// Render a centered modal overlay (dimmed backdrop + panel) for a dialog
/// entity. Clicking the backdrop invokes `close`. `max_height`, when set, caps
/// the panel height (used for the file-selection dialog).
pub fn render_modal_overlay<E: Render + 'static>(
    dialog: &Entity<E>,
    close: fn(&mut MainPanel, &mut Context<MainPanel>),
    max_height: Option<Pixels>,
    cx: &mut Context<MainPanel>,
) -> impl IntoElement {
    let theme = cx.theme();
    let mut panel = v_flex()
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
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
    if let Some(h) = max_height {
        panel = panel.max_h(h);
    }
    div()
        .absolute()
        .top_0()
        .left_0()
        .size_full()
        .bg(theme.muted)
        .opacity(0.8)
        .child(panel)
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, _, cx| close(this, cx)),
        )
}

/// Build an [`AddTorrentEntry`] by resolving the torrent's file list via
/// `list_only`. If the file list can't be resolved (e.g. a magnet that can't
/// be fetched right now), the entry is created with no files so it will be
/// added with all files selected.
pub async fn build_entry(
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
    let output_folder = PathBuf::from(response.output_folder);
    let files: Vec<FileRow> = response
        .details
        .files
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(idx, f)| {
            // Reconstruct the on-disk path from the output folder and the
            // torrent's relative file components, then check if it exists.
            let mut full_path = output_folder.clone();
            for component in &f.components {
                full_path.push(component);
            }
            let exists = !f.attributes.padding && full_path.exists();
            FileRow {
                file_index: idx,
                name: f.name,
                length: f.length,
                included: true,
                exists,
                progress: 0,
            }
        })
        .collect();
    Ok(AddTorrentEntry {
        source,
        name,
        files,
    })
}

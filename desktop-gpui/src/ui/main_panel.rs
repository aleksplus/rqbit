use gpui::{div, prelude::*, px, rgb};
use gpui_component::{DataTable, Column};

use crate::state::SharedState;
use librqbit::{TorrentDetailsResponse, TorrentStats};

/// Simple main panel that lists torrents and shows minimal actions.
pub struct MainPanel {
    /// Cached list of torrent details.
    torrents: Vec<TorrentDetailsResponse>,
}

impl Default for MainPanel {
    fn default() -> Self {
        Self { torrents: Vec::new() }
    }
}

impl Render for MainPanel {
    fn render(&mut self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        if self.torrents.is_empty() {
            let ipc = cx.ipc();
            // Fetch with stats to have progress info
            self.torrents = ipc.torrent_list().torrents;
        }

        let columns = vec![
            Column::new("ID", |t: &TorrentDetailsResponse| t.id.unwrap_or(0).to_string()),
            Column::new("Name", |t: &TorrentDetailsResponse| t.name.clone().unwrap_or_default()),
            Column::new("Pieces", |t: &TorrentDetailsResponse| t.total_pieces.to_string()),
        ];

        DataTable::new()
            .columns(columns)
            .rows(self.torrents.clone())
    }
}
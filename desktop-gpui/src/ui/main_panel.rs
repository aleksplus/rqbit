// desktop‑gpui/src/ui/main_panel.rs

use gpui::{Div, IntoElement, prelude::*};
use gpui_component::{Column, DataTable};

use crate::state::SharedState;
use futures::executor::block_on;
use librqbit::{TorrentDetailsResponse, TorrentListResponse};

/// Main panel that displays the list of torrents.
pub struct MainPanel {
    /// Cached list of torrent details fetched from the REST API.
    torrents: Vec<TorrentDetailsResponse>,
}

impl Default for MainPanel {
    fn default() -> Self {
        Self {
            torrents: Vec::new(),
        }
    }
}

impl Render for MainPanel {
    fn render(&mut self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        if self.torrents.is_empty() {
            // Fetch torrents via the HTTP client from global state.
            let client = cx.global::<SharedState>().http_client.clone();
            // Demo: block on the future; in real code use cx.spawn.
            let fut = async { client.list_torrents(true).await.unwrap() };
            self.torrents = block_on(fut).torrents; // `block_on` is only for demonstration
        }

        let columns = vec![
            Column::new("ID", |t: &TorrentDetailsResponse| {
                t.id.unwrap_or(0).to_string()
            }),
            Column::new("Name", |t: &TorrentDetailsResponse| {
                t.name.clone().unwrap_or_default()
            }),
            Column::new("Pieces", |t: &TorrentDetailsResponse| {
                t.total_pieces.to_string()
            }),
        ];

        DataTable::new()
            .columns(columns)
            .rows(self.torrents.clone())
    }
}

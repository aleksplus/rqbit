use gpui::{IntoElement, Window, prelude::*};
use gpui_component::table::{Column, DataTable, TableState};
use librqbit::api::{ApiTorrentListOpts, TorrentDetailsResponse};

use crate::state::StateShared;

/// Main panel that displays the list of torrents.
pub struct MainPanel {
    /// Cached list of torrent details fetched from the REST API.
    torrents: Vec<TorrentDetailsResponse>,
    columns: Vec<Column>,
}

// impl Default for MainPanel {
//     fn default() -> Self {
//         Self {
//             torrents: Vec::new(),
//         }
//     }
// }

impl MainPanel {
    fn new(window: &Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).default_value("Hello 世界"));
        Self { input }
    }
}

impl Render for MainPanel {
    // fn main(|cx|) {
    //     let delegate = Render::new();
    //     let state = cx.new(|cx| TableState::new(delegate, window, cx));

    //     DataTable::new(&state)
    //         .columns(columns)
    //         .rows(self.torrents.clone())
    // }

    // pub fn new() -> Self {
    //     Self {
    //         torrents: Vec::new(),
    //         columns: vec![
    //             Column::new("ID", |t: &TorrentDetailsResponse| {
    //                 t.id.unwrap_or(0).to_string()
    //             }),
    //             Column::new("Name", |t: &TorrentDetailsResponse| {
    //                 t.name.clone().unwrap_or_default()
    //             }),
    //             Column::new("Pieces", |t: &TorrentDetailsResponse| {
    //                 t.total_pieces.to_string()
    //             }),
    //         ],
    //     }
    // }
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // fn render(&mut self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        if self.torrents.is_empty() {}

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

        DataTable::new(self)
            .columns(columns)
            .rows(self.torrents)
    }
}

//! Browser implementation of the shared UI's [`UiHost`]: no file dialogs, no
//! OS integration; the theme is session-local and the playlist row is
//! unavailable (the daemon's HTTP media endpoints are not part of this
//! protocol).

use std::future::Future;

use gpui::App;
use rqbit_ui::host::{PickedTorrentFile, UiCapabilities, UiHost};
use rqbit_ui::ui::add_torrent_dialog::AddTorrentEntry;

pub struct WebHost;

impl UiHost for WebHost {
    fn capabilities(&self) -> UiCapabilities {
        UiCapabilities::remote()
    }

    fn version(&self) -> String {
        "web".to_string()
    }

    fn pick_torrent_files(
        &self,
        _cx: &mut App,
    ) -> std::pin::Pin<Box<dyn Future<Output = Result<Vec<PickedTorrentFile>, String>> + 'static>>
    {
        Box::pin(async {
            Err("picking .torrent files is not supported in the browser client yet; \
                 use Add Magnet or a URL"
                .to_string())
        })
    }

    fn adjust_file_selection<'a>(
        &'a self,
        _entry: &'a mut AddTorrentEntry,
    ) -> std::pin::Pin<Box<dyn Future<Output = Vec<String>> + 'a>> {
        Box::pin(async { Vec::new() })
    }
}

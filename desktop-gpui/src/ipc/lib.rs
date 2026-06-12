use std::sync::Arc;

// Re-export the core library API for IPC consumers.
pub use librqbit::Api;

/// Thin wrapper around the library `Api` that will be exposed over IPC.
/// The actual JSON‑RPC dispatch logic lives in `server.rs`.
#[derive(Clone)]
pub struct IpcService {
    api: Arc<Api>,
}

impl IpcService {
    /// Create a new service from the provided library `Api`.
    pub fn new(api: Arc<Api>) -> Self {
        Self { api }
    }

    /// Create a new torrent from a magnet link.
    pub async fn create_magnet(&self, magnet: String) -> Result<u64, librqbit::ApiError> {
        use librqbit::{AddTorrent, AddTorrentOptions};
        let add = AddTorrent::new(magnet)
            .with_options(AddTorrentOptions::default());
        let resp = self.api
            .api_add_torrent(add, None)
            .await?;
        match resp {
            librqbit::ApiAddTorrentResponse::Added(id, _) => Ok(id),
            _ => Err(librqbit::ApiError::from("Failed to add torrent")),
        }
    }

    /// Get the live state of a torrent.
    pub async fn get_state(&self, tid: u64) -> Result<librqbit::torrent_state::LiveStats, librqbit::ApiError> {
        let handle = self.api.session().get(&tid).ok_or_else(||
            librqbit::ApiError::from("Torrent not found"),
        )?;
        Ok(handle.stats().await?)
    }
}

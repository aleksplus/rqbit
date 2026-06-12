use std::sync::Arc;

pub use librqbit::Api;
use librqbit::{
    AddTorrent, AddTorrentOptions, ApiAddTorrentResponse, ApiError, ApiTorrentListOpts,
    EmptyJsonResponse, SessionStatsSnapshot, TorrentIdOrHash, TorrentListResponse, TorrentStats,
    api::PeerStatsFilter,
    http_api_types::PeerStatsSnapshot,
};

#[derive(Clone)]
pub struct IpcService {
    api: Arc<Api>,
}

impl IpcService {
    pub fn new(api: Arc<Api>) -> Self {
        Self { api }
    }

    pub async fn create_magnet(&self, magnet: String) -> Result<u64, ApiError> {
        let add = AddTorrent::new(magnet).with_options(AddTorrentOptions::default());
        let resp = self.api.api_add_torrent(add, None).await?;
        match resp {
            ApiAddTorrentResponse::Added(id, _) => Ok(id),
            _ => Err(ApiError::from("Failed to add torrent")),
        }
    }

    pub async fn get_state(&self, tid: u64) -> Result<TorrentStats, ApiError> {
        let handle = self
            .api
            .session()
            .get(tid.into())
            .ok_or_else(|| ApiError::from("Torrent not found"))?;
        Ok(handle.stats())
    }

    pub fn set_peer_limit(&self, tid: u64, limit: usize) -> Result<(), ApiError> {
        let handle = self
            .api
            .session()
            .get(tid.into())
            .ok_or_else(|| ApiError::from("Torrent not found"))?;
        handle.shared().options.peer_limit = Some(limit);
        Ok(())
    }

    pub fn get_peer_list(
        &self,
        tid: u64,
        filter: Option<PeerStatsFilter>,
    ) -> Result<PeerStatsSnapshot, ApiError> {
        let handle = self
            .api
            .session()
            .get(tid.into())
            .ok_or_else(|| ApiError::from("Torrent not found"))?;
        let live = handle
            .live()
            .ok_or_else(|| ApiError::from("Torrent is not live"))?;
        Ok(live.per_peer_stats_snapshot(
            filter.unwrap_or_else(|| PeerStatsFilter {
                state: librqbit::http_api_types::PeerStatsFilterState::Live,
            }),
        ))
    }

    pub fn dht_peer_addr(&self) -> Result<librqbit::dht::DhtStats, ApiError> {
        self.api.api_dht_stats()
    }

    pub fn stats(&self) -> SessionStatsSnapshot {
        self.api.api_session_stats()
    }

    pub fn get_session_config(&self) -> Result<serde_json::Value, ApiError> {
        Ok(serde_json::json!({
            "session_id": self.api.session().id()
        }))
    }

    pub async fn seed_magnet(
        &self,
        magnet: String,
        output_folder: Option<String>,
    ) -> Result<u64, ApiError> {
        let mut opts = AddTorrentOptions::default();
        if let Some(output_folder) = output_folder {
            opts.output_folder = Some(output_folder);
        }
        let add = AddTorrent::new(magnet).with_options(opts);
        let resp = self.api.api_add_torrent(add, None).await?;
        match resp {
            ApiAddTorrentResponse::Added(id, _) => Ok(id),
            _ => Err(ApiError::from("Failed to seed torrent")),
        }
    }

    pub fn tracker_list(&self, tid: u64) -> Result<Vec<String>, ApiError> {
        let handle = self
            .api
            .session()
            .get(tid.into())
            .ok_or_else(|| ApiError::from("Torrent not found"))?;
        Ok(handle
            .shared()
            .trackers
            .iter()
            .map(|url| url.to_string())
            .collect())
    }

    pub async fn delete_torrent(&self, tid: u64) -> Result<EmptyJsonResponse, ApiError> {
        self.api
            .api_torrent_action_delete(tid.into())
            .await
            .map_err(ApiError::from)
    }

    pub fn torrent_list(&self) -> TorrentListResponse {
        self.api
            .api_torrent_list_ext(ApiTorrentListOpts { with_stats: true })
    }

    pub fn peer_stats(
        &self,
        tid: u64,
        filter: Option<PeerStatsFilter>,
    ) -> Result<PeerStatsSnapshot, ApiError> {
        self.get_peer_list(tid, filter)
    }

    pub fn torrent_stats(&self, tid: u64) -> Result<TorrentStats, ApiError> {
        self.get_state(tid)
    }

    pub fn api(&self) -> &Api {
        &self.api
    }
}

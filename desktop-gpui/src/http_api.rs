use std::sync::{Arc, RwLock};

use anyhow::Context;
use gpui_http_client::{Client, FormData};
use librqbit::api::{TorrentDetailsResponse, TorrentListResponse, TorrentStats, PeerStatsSnapshot};
use librqbit::http_api_types::{PeerStatsFilter};

/// A lightweight HTTP client that talks to the rqbit REST API.
#[derive(Clone)]
pub struct HttpClient {
    base_url: String,
    client: Client,
}

impl HttpClient {
    /// Create a new client. `base_url` should be something like
    /// "http://127.0.0.1:3030".
    pub fn new(base_url: String) -> Self {
        Self { base_url, client: Client::new() }
    }

    /// GET /torrents?with_stats=true|false
    pub async fn list_torrents(&self, with_stats: bool) -> anyhow::Result<TorrentListResponse> {
        let mut url = format!("{}/torrents", self.base_url);
        if with_stats {
            url.push_str("?with_stats=true");
        }
        let resp = self.client.get(&url).await?;
        let data: TorrentListResponse = serde_json::from_slice(&resp)?;
        Ok(data)
    }

    /// GET /torrents/{id}
    pub async fn get_torrent_details(&self, id: usize) -> anyhow::Result<TorrentDetailsResponse> {
        let url = format!("{}/torrents/{}", self.base_url, id);
        let resp = self.client.get(&url).await?;
        let data: TorrentDetailsResponse = serde_json::from_slice(&resp)?;
        Ok(data)
    }

    /// GET /torrents/{id}/stats/v1
    pub async fn get_torrent_stats(&self, id: usize) -> anyhow::Result<TorrentStats> {
        let url = format!("{}/torrents/{}/stats/v1", self.base_url, id);
        let resp = self.client.get(&url).await?;
        let data: TorrentStats = serde_json::from_slice(&resp)?;
        Ok(data)
    }

    /// GET /torrents/{id}/peer_stats?state=live
    pub async fn get_peer_stats(&self, id: usize, filter_state: PeerStatsFilter) -> anyhow::Result<PeerStatsSnapshot> {
        let url = format!("{}/torrents/{}/peer_stats?state={}", self.base_url, id, filter_state);
        let resp = self.client.get(&url).await?;
        let data: PeerStatsSnapshot = serde_json::from_slice(&resp)?;
        Ok(data)
    }

    /// POST /torrents?overwrite=true&list_only=false&only_files=...
    /// For brevity, this example only implements a minimal upload of URL.
    pub async fn upload_torrent_from_url(&self, url: &str) -> anyhow::Result<()> {
        let api_url = format!("{}/torrents?overwrite=true", self.base_url);
        let mut form = FormData::new();
        form.append("url", url);
        let _resp = self.client.post(&api_url, Some(form)).await?;
        Ok(())
    }

    /// Other actions (pause, start, delete) can be added similarly.
}

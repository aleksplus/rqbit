use std::sync::Arc;

use anyhow::Context;
use gpui_http_client::{Client, Response};
use librqbit::api::{PeerStatsSnapshot, TorrentDetailsResponse, TorrentListResponse, TorrentStats};
use librqbit::http_api_types::PeerStatsFilter;
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
        let client = Client::new();
        Self { base_url, client }
    }

    async fn check_response(
        mut r: gpui_http_client::Response,
    ) -> anyhow::Result<gpui_http_client::Response> {
        if r.status().is_success() {
            return Ok(r);
        }
        let status = r.status();
        let url = r.url().clone();
        // Try to read body for error message
        let body = r
            .text()
            .await
            .unwrap_or_else(|_| "<unable to read body>".to_string());
        anyhow::bail!("{} -> {}: {}", url, status, body)
    }

    async fn json_response<T: serde::de::DeserializeOwned + std::any::Any>(
        r: gpui_http_client::Response,
    ) -> anyhow::Result<T> {
        let r = Self::check_response(r).await?;
        let bytes = r.bytes().await.map_err(|e| anyhow::anyhow!(e))?;
        let val: T = serde_json::from_slice(&bytes)?;
        Ok(val)
    }

    pub async fn list_torrents(&self, with_stats: bool) -> anyhow::Result<TorrentListResponse> {
        let mut url = format!("{}/torrents", self.base_url);
        if with_stats {
            url.push_str("?with_stats=true");
        }
        let resp = self.client.get(&url).send().await?;
        Self::json_response(resp).await
    }

    pub async fn torrent_details(&self, id: usize) -> anyhow::Result<TorrentDetailsResponse> {
        let url = format!("{}/torrents/{}", self.base_url, id);
        let resp = self.client.get(&url).send().await?;
        Self::json_response(resp).await
    }

    pub async fn pause(&self, id: usize) -> anyhow::Result<()> {
        let url = format!("{}/torrents/{}/pause", self.base_url, id);
        let resp = self.client.post(&url).send().await?;
        Self::check_response(resp).await.map(|_| ())
    }

    pub async fn delete(&self, id: usize) -> anyhow::Result<()> {
        let url = format!("{}/torrents/{}/delete", self.base_url, id);
        let resp = self.client.post(&url).send().await?;
        Self::check_response(resp).await.map(|_| ())
    }

    pub async fn start(&self, id: usize) -> anyhow::Result<()> {
        let url = format!("{}/torrents/{}/start", self.base_url, id);
        let resp = self.client.post(&url).send().await?;
        Self::check_response(resp).await.map(|_| ())
    }
}

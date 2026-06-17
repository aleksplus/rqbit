use std::sync::Arc;

use anyhow::{Context, Result};
use gpui_http_client::{HttpClient as GpuiClient, Response};
use librqbit::api::{TorrentDetailsResponse, TorrentListResponse};
use url::Url;

/// A lightweight HTTP client that talks to the rqbit REST API.
#[derive(Clone)]
pub struct HttpClient {
    base_url: Url,
    client: GpuiClient,
}

impl HttpClient {
    /// Create a new client. `base_url` should be something like
    /// "http://127.0.0.1:3030".
    pub fn new(base_url: String) -> Result<Self> {
        let base = Url::parse(&base_url)
            .with_context(|| format!("invalid base URL: {}", base_url))?;
        let client = GpuiClient::new();
        Ok(Self { base_url: base, client })
    }

    async fn check_response<T>(mut r: Response<T>) -> Result<Response<T>> {
        if r.status().is_success() {
            return Ok(r);
        }
        let status = r.status();
        let url = r.url().clone();
        // Try to read body for error message
        let body = r.text().await.unwrap_or_else(|_| "<unable to read body>".to_string());
        anyhow::bail!("{} -> {}: {}", url, status, body)
    }

    async fn json_response<T: serde::de::DeserializeOwned, U: T>(r: Response<U>) -> Result<T> {
        let r = Self::check_response(r).await?;
        let bytes = r.bytes().await.map_err(|e| anyhow::anyhow!(e))?;
        let val: T = serde_json::from_slice(&bytes)?;
        Ok(val)
    }

    pub async fn list_torrents(&self, with_stats: bool) -> Result<TorrentListResponse> {
        let mut url = self.base_url.clone();
        url.path_segments_mut()
            .map_err(|_| anyhow::anyhow!("cannot modify URL path"))?
            .push("torrents");
        if with_stats {
            url.query_pairs_mut().append_pair("with_stats", "true");
        }
        let resp = self.client.get(url.as_str()).send().await?;
        Self::json_response::<TorrentListResponse>(resp).await
    }

    pub async fn torrent_details(&self, id: usize) -> Result<TorrentDetailsResponse> {
        let mut url = self.base_url.clone();
        url.path_segments_mut()
            .map_err(|_| anyhow::anyhow!("cannot modify URL path"))?
            .push("torrents")
            .push(&id.to_string());
        let resp = self.client.get(url.as_str()).send().await?;
        Self::json_response::<TorrentDetailsResponse>(resp).await
    }

    async fn torrent_action(&self, id: usize, action: &str) -> Result<()> {
        let mut url = self.base_url.clone();
        url.path_segments_mut()
            .map_err(|_| anyhow::anyhow!("cannot modify URL path"))?
            .push("torrents")
            .push(&id.to_string())
            .push(action);
        let resp = self.client.post(url.as_str()).send().await?;
        Self::check_response(resp).await.map(|_| ())
    }

    pub async fn pause(&self, id: usize) -> Result<()> {
        self.torrent_action(id, "pause").await
    }

    pub async fn delete(&self, id: usize) -> Result<()> {
        self.torrent_action(id, "delete").await
    }

    pub async fn start(&self, id: usize) -> Result<()> {
        self.torrent_action(id, "start").await
    }
}

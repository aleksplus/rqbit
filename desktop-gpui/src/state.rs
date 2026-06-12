use std::sync::{Arc, RwLock};

use crate::http_api::HttpClient;
use librqbit::{Api, ApiError};
#[derive(Clone)]
pub struct SharedState {
    pub config: crate::config::RqbitDesktopConfig,
    pub http_client: Arc<HttpClient>,
}

pub async fn init_shared_state(
    init_logging: crate::tracing_subscriber_config_utils::InitLoggingResult,
) -> anyhow::Result<SharedState> {
    // Load config as before (keep the same logic)
    let config_path = directories::ProjectDirs::from("com", "rqbit", "desktop")
        .expect("directories::ProjectDirs::from")
        .config_dir()
        .join("config.json");
    let config: crate::config::RqbitDesktopConfig = {
        let rdr = std::io::BufReader::new(std::fs::File::open(&config_path)?);
        let mut cfg: crate::config::RqbitDesktopConfig = serde_json::from_reader(rdr)?;
        cfg.persistence.fix_backwards_compat();
        cfg
    };

    // Create HttpClient
    let base_url = if let Some(addr) = config.http_api.listen_addr {
        // Convert "127.0..:3030" to http URL
        format!("http://{}", addr)
    } else {
        panic!("HTTP API listen address not configured");
    };
    let http_client = Arc::new(HttpClient::new(base_url));

    Ok(SharedState {
        config,
        http_client,
    })
}

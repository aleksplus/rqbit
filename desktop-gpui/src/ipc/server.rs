use std::{os::unix::net::UnixListener, path::PathBuf, sync::Arc};

use anyhow::Context;
use ring::hmac;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::{io::{AsyncReadExt, AsyncWriteExt}, net::UnixStream, runtime::Runtime};

use crate::ipc::{IpcService, Api};

pub const API_VERSION: &str = "1.0";
pub const DEFAULT_SOCKET_PATH: &str = "/tmp/rqlib-ipc.sock";

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub cmd: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub payload: Value,
    #[serde(default)]
    pub auth_token: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: &'static str,
    pub result: Value,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcError {
    pub jsonrpc: &'static str,
    pub error: RpcError,
}

#[derive(Debug, Serialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

#[derive(Clone)]
pub struct AuthConfig {
    secret: Arc<Vec<u8>>,
}

impl AuthConfig {
    pub fn new(secret: Vec<u8>) -> Self {
        Self {
            secret: Arc::new(secret),
        }
    }

    pub fn verify(&self, token: &str, payload: &Value) -> bool {
        let mut mac = hmac::Context::new(&hmac::HMAC_SHA256);
        mac.update(self.secret.as_slice());
        mac.update(serde_json::to_vec(payload).unwrap_or_default().as_slice());
        let expected = hex::encode(mac.sign().as_ref());
        token == expected
    }
}

pub fn default_version() -> String {
    API_VERSION.to_string()
}

pub fn load_secret(path: impl AsRef<std::path::Path>) -> anyhow::Result<Vec<u8>> {
    let data = std::fs::read(path)?;
    let value: toml::Value = toml::from_slice(&data)?;
    let secret_hex = value
        .get("secret")
        .and_then(|v| v.as_str())
        .context("missing secret in secrets.toml")?;
    Ok(hex::decode(secret_hex)?)
}

pub fn run_server(api: Arc<Api>) -> anyhow::Result<()> {
    run_server_with_path(api, DEFAULT_SOCKET_PATH.into())
}

pub fn run_server_with_path(api: Arc<Api>, socket_path: PathBuf) -> anyhow::Result<()> {
    let _ = std::fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path)
        .with_context(|| format!("failed to bind socket {}", socket_path.display()))?;
    listener.set_nonblocking(true)?;

    let rt = Runtime::new().context("tokio runtime creation failed")?;
    rt.block_on(async move {
        let service = IpcService::new(api);
        loop {
            match listener.accept() {
                Ok((stream, _addr)) => {
                    let svc = service.clone();
                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(stream, svc).await {
                            eprintln!("ipc connection error: {e:#}");
                        }
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
                Err(e) => eprintln!("ipc accept error: {e:?}"),
            }
        }
    })
}

async fn handle_connection(mut stream: UnixStream, svc: IpcService) -> anyhow::Result<()> {
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await?;
    let req: JsonRpcRequest = serde_json::from_slice(&buf)?;
    let resp = dispatch_request(req, svc).await;
    let resp_json = serde_json::to_vec(&resp)?;
    stream.write_all(&resp_json).await?;
    stream.shutdown().await?;
    Ok(())
}

pub async fn dispatch_request(req: JsonRpcRequest, svc: IpcService) -> Value {
    if req.version != API_VERSION {
        return error_response(-32600, "unsupported api version");
    }
    let payload = req.payload;
    let result = match req.cmd.as_str() {
        "create_magnet" => {
            let magnet: String = match serde_json::from_value(payload.clone()) {
                Ok(v) => v,
                Err(e) => return error_response(-32602, format!("invalid payload: {e}")),
            };
            svc.create_magnet(magnet).await.map(|id| json!(id))
        }
        "get_state" => {
            let tid: u64 = match serde_json::from_value(payload.clone()) {
                Ok(v) => v,
                Err(e) => return error_response(-32602, format!("invalid payload: {e}")),
            };
            svc.get_state(tid).await.map(|stats| json!(stats))
        }
        "set_peer_limit" => {
            let payload: SetPeerLimitPayload = match serde_json::from_value(payload.clone()) {
                Ok(v) => v,
                Err(e) => return error_response(-32602, format!("invalid payload: {e}")),
            };
            svc.set_peer_limit(payload.tid, payload.limit).map(|_| Value::Null)
        }
        "get_peer_list" => {
            let tid: u64 = match serde_json::from_value(payload.clone()) {
                Ok(v) => v,
                Err(e) => return error_response(-32602, format!("invalid payload: {e}")),
            };
            svc.get_peer_list(tid, None).map(|peers| json!(peers))
        }
        "dht_peer_addr" => svc.dht_peer_addr().map(|stats| json!(stats)),
        "stats" => Ok(json!(svc.stats())),
        "get_session_config" => svc.get_session_config(),
        "seed_magnet" => {
            let payload: SeedMagnetPayload = match serde_json::from_value(payload.clone()) {
                Ok(v) => v,
                Err(e) => return error_response(-32602, format!("invalid payload: {e}")),
            };
            svc.seed_magnet(payload.magnet, payload.output_folder)
                .await
                .map(|id| json!(id))
        }
        "tracker_list" => {
            let tid: u64 = match serde_json::from_value(payload.clone()) {
                Ok(v) => v,
                Err(e) => return error_response(-32602, format!("invalid payload: {e}")),
            };
            svc.tracker_list(tid).map(|trackers| json!(trackers))
        }
        "delete_torrent" => {
            let tid: u64 = match serde_json::from_value(payload.clone()) {
                Ok(v) => v,
                Err(e) => return error_response(-32602, format!("invalid payload: {e}")),
            };
            svc.delete_torrent(tid).await.map(|resp| json!(resp))
        }
        _ => Err(librqbit::ApiError::from("unknown cmd")),
    };
    match result {
        Ok(value) => json!({
            "jsonrpc": "2.0",
            "result": value,
        }),
        Err(e) => error_response(e.status().as_u16() as i32, e.to_string()),
    }
}

pub fn error_response(code: i32, message: impl Into<String>) -> Value {
    json!({
        "jsonrpc": "2.0",
        "error": {
            "code": code,
            "message": message.into(),
        },
    })
}

#[derive(Debug, Deserialize)]
pub struct SetPeerLimitPayload {
    pub tid: u64,
    pub limit: usize,
}

#[derive(Debug, Deserialize)]
pub struct SeedMagnetPayload {
    pub magnet: String,
    pub output_folder: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auth_verifies_payload_hmac() {
        let secret = b"secret".to_vec();
        let auth = AuthConfig::new(secret.clone());
        let payload = json!({"tid": 1});
        let mut mac = hmac::Context::new(&hmac::HMAC_SHA256);
        mac.update(secret.as_slice());
        mac.update(serde_json::to_vec(&payload).unwrap().as_slice());
        let token = hex::encode(mac.sign().as_ref());
        assert!(auth.verify(&token, &payload));
        assert!(!auth.verify(&token, &json!({"tid": 2})));
    }

    #[test]
    fn rejects_unknown_version() {
        let req = JsonRpcRequest {
            cmd: "stats".to_string(),
            version: "0.1".to_string(),
            payload: Value::Null,
            auth_token: None,
        };
        assert_eq!(req.version, "0.1");
    }
}

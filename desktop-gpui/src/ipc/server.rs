use std::{os::unix::net::UnixListener, sync::Arc};

use tokio::{net::UnixStream, runtime::Runtime};
use serde_json::Value;

use crate::ipc::{IpcService, Api};

/// Path to the Unix domain socket.
const SOCKET_PATH: &str = "/tmp/rqlib-ipc.sock";

/// Load the HMAC secret from a TOML file.
fn load_secret() -> anyhow::Result<Vec<u8>> {
    use std::{fs, path::Path};
    let data = fs::read("desktop-gpui/secrets.toml")?;
    let value: toml::Value = toml::from_slice(&data)?;
    let secret_hex = value
        .get("secret")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing secret in secrets.toml"))?;
    let secret = hex::decode(secret_hex)?;
    Ok(secret)
}

/// Starts the IPC server and blocks until termination.
pub fn run_server(api: Arc<Api>) {
    // Ensure the socket does not exist.
    let _ = std::fs::remove_file(SOCKET_PATH);
    let listener = UnixListener::bind(SOCKET_PATH).expect("Failed to bind socket");
    listener.set_nonblocking(true).expect("set_nonblocking failed");

    let rt = Runtime::new().expect("tokio runtime creation failed");
    rt.block_on(async move {
        let service = IpcService::new(api);
        loop {
            match listener.accept() {
                Ok((stream, _addr)) => {
                    let svc = service.clone();
                    tokio::spawn(async move { handle_connection(stream, svc).await });
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
                }
                Err(e) => eprintln!("accept error: {:?}", e),
            }
        }
    });
}

async fn handle_connection(mut stream: UnixStream, svc: IpcService, secret: Vec<u8>) {
    // Simplified: read a single line JSON request and respond.
    let mut buf = Vec::new();
    if stream.readable().await.is_ok() {
        use tokio::io::AsyncReadExt;
        let _ = stream.read_to_end(&mut buf).await;
    }
    let req: Value = serde_json::from_slice(&buf).unwrap_or_default();
    // Verify HMAC
    if let Some(token) = req.get("auth_token").and_then(|v| v.as_str()) {
        let payload = req.get("payload").unwrap_or(&Value::Null);
        let mut mac = ring::hmac::Context::new(&ring::hmac::HMAC_SHA256);
        mac.update(secret.as_slice());
        mac.update(serde_json::to_vec(payload).unwrap_or_default().as_slice());
        let expected = hex::encode(mac.sign().as_ref());
        if token != expected {
            let resp = serde_json::json!({"error": "unauthenticated"});
            let _ = stream.write_all(resp.to_string().as_bytes()).await;
            return;
        }
    } else {
        let resp = serde_json::json!({"error": "missing auth_token"});
        let _ = stream.write_all(resp.to_string().as_bytes()).await;
        return;
    }
    let cmd = req.get("cmd").and_then(|v| v.as_str()).unwrap_or("");
    let resp = match cmd {
        "create_magnet" => {
            let magnet = req.get("payload").and_then(|v| v.as_str()).unwrap_or("");
            svc.create_magnet(magnet.to_string())
        }
        "get_state" => {
            let tid = req.get("payload").and_then(|v| v.as_u64()).unwrap_or(0);
            svc.get_state(tid)
        }
        _ => Err(librqbit::ApiError::from("unknown cmd")),
    };
    let resp_json = serde_json::to_string(&resp).unwrap_or_default();
    use tokio::io::AsyncWriteExt;
    let _ = stream.write_all(resp_json.as_bytes()).await;
}

use std::{fs, path::PathBuf, sync::Arc};

use anyhow::Context;
use ring::hmac;
use serde_json::{json, Value};
use tokio::{net::UnixListener, io::{AsyncReadExt, AsyncWriteExt, DuplexStream}, runtime::Runtime};

use desktop_gpui::ipc::{IpcService, run_server};

/// Create a temporary secrets.toml and return the secret bytes
fn load_secret(tmp_dir: &PathBuf) -> anyhow::Result<Vec<u8>> {
    let secret_hex = "deadbeefcafebabe";
    let secrets_path = tmp_dir.join("secrets.toml");
    fs::write(&secrets_path, format!("secret = "{}", secret_hex))?;
    Ok(hex::decode(secret_hex)?)
}

fn hmac_for(secret: &[u8], payload: &Value) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
    hex::encode(hmac::sign(&key, &serde_json::to_vec(payload).unwrap()))
}

/// Helper that creates a mock duplex stream, starts the server in a background task
async fn setup_server(api: Arc<librqbit::Api>, secret: Vec<u8>) -> DuplexStream {
    // Spawn server in background
    let svc = IpcService::new(api);
    tokio::spawn(async move { run_server(svc) });

    // Give the server a moment to bind
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Create a duplex stream pair
    let (client, server) = tokio::io::duplex(64);
    // Forward server side to UnixListener by binding to socket path and accepting
    let listener = UnixListener::bind("/tmp/rqlib-ipc.sock").expect("listener bind");
    tokio::spawn(async move {
        if let Ok((mut stream, _)) = listener.accept() {
            // pipe data between duplex server side and actual socket
            let mut server_side = server;
            let (mut to_socket, mut from_socket) = stream.split();
            tokio::select! {
                _ = tokio::io::copy(&mut server_side, &mut to_socket) => {},
                _ = tokio::io::copy(&mut from_socket, &mut server_side) => {},
            }
        }
    });

    client
}

async fn send_request(mut stream: DuplexStream, cmd: &str, payload: Value, secret: &[u8]) -> Value {
    let req = json!({
        "cmd": cmd,
        "payload": payload,
        "auth_token": hmac_for(secret, &payload),
    });
    stream.write_all(req.to_string().as_bytes()).await.unwrap();
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.unwrap();
    serde_json::from_slice::<Value>(&buf).unwrap()
}

// Helper to create a minimal Api with an in‑memory session
async fn build_api(tmp_dir: &PathBuf) -> Arc<librqbit::Api> {
    let session = librqbit::Session::new_with_opts(
        tmp_dir.to_str().unwrap(),
        librqbit::SessionOptions::default(),
    )
    .await
    .context("session")?;
    Ok(Arc::new(librqbit::Api::new(session, None, None)))
}

#[tokio::test]
async fn test_ipc_methods() -> anyhow::Result<()> {
    let tmp_dir = tempfile::tempdir()?;
    let secret = load_secret(tmp_dir.path())?;
    let api = build_api(tmp_dir.path()).await?;

    // Add a torrent for state tests
    let add_resp = api.api_add_torrent(
        librqbit::AddTorrent::new("magnet:?xt=urn:btih:abcd"),
        None,
    )
    .await?;
    let torrent_id = match add_resp {
        librqbit::ApiAddTorrentResponse::Added(id, _) => id,
        _ => return Err(anyhow::anyhow!("add failed")),
    };

    let stream = setup_server(api.clone(), secret.clone()).await;

    // create_magnet
    let resp = send_request(stream.clone(), "create_magnet", json!({"magnet": "magnet:?xt=urn:btih:1234"}), &secret).await;
    assert!(resp.get("tid").is_some());

    // get_state
    let resp = send_request(stream.clone(), "get_state", json!({"tid": torrent_id}), &secret).await;
    assert!(resp.get("state").is_some());

    // set_peer_limit
    let _ = send_request(stream.clone(), "set_peer_limit", json!({"tid": torrent_id, "limit": 50}), &secret).await;

    // get_peer_list
    let _ = send_request(stream.clone(), "get_peer_list", json!({"tid": torrent_id}), &secret).await;

    // dht_peer_addr
    let _ = send_request(stream.clone(), "dht_peer_addr", json!({}), &secret).await;

    // stats
    let _ = send_request(stream.clone(), "stats", json!({}), &secret).await;

    // get_session_config
    let _ = send_request(stream.clone(), "get_session_config", json!({}), &secret).await;

    // seed_magnet
    let _ = send_request(stream.clone(), "seed_magnet", json!({"magnet": "magnet:?xt=urn:btih:seed", "output_folder": tmp_dir.path().to_str() }), &secret).await;

    // tracker_list
    let _ = send_request(stream.clone(), "tracker_list", json!({"tid": torrent_id}), &secret).await;

    // delete_torrent
    let _ = send_request(stream.clone(), "delete_torrent", json!({"tid": torrent_id}), &secret).await;

    // torrent_list
    let _ = send_request(stream.clone(), "torrent_list", json!({}), &secret).await;

    // peer_stats
    let _ = send_request(stream.clone(), "peer_stats", json!({"tid": torrent_id}), &secret).await;

    // torrent_stats
    let _ = send_request(stream.clone(), "torrent_stats", json!({"tid": torrent_id}), &secret).await;

    Ok(())
}

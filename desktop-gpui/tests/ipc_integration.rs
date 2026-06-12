use std::{fs, path::PathBuf, sync::Arc};

use anyhow::Context;
use ring::hmac;
use serde_json::{json, Value};
use tokio::{net::UnixStream, runtime::Runtime, io::{AsyncReadExt, AsyncWriteExt}};

use desktop_gpui::ipc::{IpcService, run_server};

/// Helper to load the secret from a temporary `secrets.toml` file.
fn load_secret(path: &PathBuf) -> anyhow::Result<Vec<u8>> {
    let data = fs::read(path)?;
    let v: toml::Value = toml::from_slice(&data)?;
    let hex = v
        .get("secret")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing secret"))?;
    Ok(hex::decode(hex)?)
}

fn hmac_for(secret: &[u8], payload: &Value) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
    hex::encode(hmac::sign(&key, &serde_json::to_vec(payload).unwrap()))
}

#[tokio::test]
async fn test_ipc_create_magnet() -> anyhow::Result<()> {
    // 1. Create a temporary secrets.toml
    let tmp_dir = tempfile::tempdir()?;
    let secret_hex = "deadbeefcafebabe"; // dummy hex
    let secrets_path = tmp_dir.path().join("secrets.toml");
    fs::write(&secrets_path, format!("secret = "{}"", secret_hex))?;

    // 2. Create a minimal Api (using an in‑memory session)
    let tmp_data_dir = tmp_dir.path().join("data");
    std::fs::create_dir_all(&tmp_data_dir)?;
    let session = librqbit::Session::new_with_opts(
        tmp_data_dir.to_str().unwrap(),
        librqbit::SessionOptions::default(),
    )
    .await
    .context("session")?;

    let api = Arc::new(librqbit::Api::new(
        session,
        None,
        None,
    ));

    // 3. Start the server in background
    let svc = IpcService::new(api.clone());
    tokio::spawn(async move { run_server(svc) });

    // Give the server time to bind
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // 4. Connect to socket
    let mut stream = UnixStream::connect("/tmp/rqlib-ipc.sock").await?;

    // 5. Send signed create_magnet request
    let payload = json!({"magnet": "magnet:?xt=urn:btih:1234"});
    let request = json!({
        "cmd": "create_magnet",
        "payload": payload,
        "auth_token": hmac_for(&load_secret(&secrets_path)?, &payload),
    });
    stream.write_all(request.to_string().as_bytes()).await?;

    // 6. Read response
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await?;
    let resp: Value = serde_json::from_slice(&buf)?;
    assert!(resp.get("tid").is_some(), "expected torrent id");

    // 7. Test authentication failure
    let bad_request = json!({
        "cmd": "create_magnet",
        "payload": payload,
        "auth_token": "badtoken"
    });
    stream.write_all(bad_request.to_string().as_bytes()).await?;
    let mut bad_buf = Vec::new();
    stream.read_to_end(&mut bad_buf).await?;
    let bad_resp: Value = serde_json::from_slice(&bad_buf)?;
    assert_eq!(
        bad_resp.get("error").and_then(|e| e.as_str()),
        Some("unauthenticated")
    );

    Ok(())
}

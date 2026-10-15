//! Static web-client assets embedded into the daemon binary (see `build.rs`).
//!
//! Served on the daemon's own port at `/` — the same slot the
//! classic React webui occupies in the upstream `rqbit server`, so the
//! browser client needs no separate web server.

include!(concat!(env!("OUT_DIR"), "/generated.rs"));

pub const CONTENT_TYPE_HTML: &str = "text/html; charset=utf-8";
pub const CONTENT_TYPE_JS: &str = "text/javascript";
pub const CONTENT_TYPE_WASM: &str = "application/wasm";

/// Placeholder page served until `make web-client-build` has produced the
/// wasm payload (see `APP_BUILT`).
pub const NOT_BUILT_HTML: &[u8] = br#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="utf-8"><title>rqbit</title></head>
<body style="font-family: sans-serif; background: #111; color: #ddd; display: flex; align-items: center; justify-content: center; height: 100vh;">
  <div>
    <h2>rqbit web client is not built</h2>
    <p>Run <code>make web-client-build</code>, then restart the daemon; the
    client is embedded into this binary at compile time.</p>
    <p style="color:#888">One-time setup: rustup with the
    <code>wasm32-unknown-unknown</code> target and <code>wasm-bindgen-cli</code>
    (see the Makefile).</p>
  </div>
</body>
</html>
"#;

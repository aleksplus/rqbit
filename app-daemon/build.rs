//! Embeds the web client (web-gpui wasm payload) into the daemon binary.
//!
//! `index.html` is a source file and always present. The wasm-bindgen
//! artifacts only exist after `make web-client-build`; for missing ones this
//! script writes placeholders into OUT_DIR so `include_bytes!` always
//! resolves, exposes `APP_BUILT`, and lets the server respond with a helpful
//! placeholder page until the payload is built.

use std::{env, fs, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let web = manifest_dir
        .parent()
        .expect("app-daemon lives in the workspace root")
        .join("web-gpui");

    let index = web.join("index.html");
    let js = web.join("dist/pkg/web_gpui.js");
    let wasm = web.join("dist/pkg/web_gpui_bg.wasm");

    // Specific files, not the directory: cargo only watches directory mtimes,
    // which do not change when wasm-bindgen rewrites files in place.
    println!("cargo:rerun-if-changed={}", index.display());
    println!("cargo:rerun-if-changed={}", js.display());
    println!("cargo:rerun-if-changed={}", wasm.display());

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let js_path = existing_or_placeholder(
        &js,
        &out_dir.join("web_gpui.js"),
        b"console.warn('rqbit web client is not built; run `make web-client-build`');\n".to_vec(),
    );
    let wasm_path = existing_or_placeholder(&wasm, &out_dir.join("web_gpui_bg.wasm"), Vec::new());
    let app_built = js.is_file() && wasm.is_file();

    fs::write(
        out_dir.join("generated.rs"),
        format!(
            "pub const INDEX_HTML: &[u8] = include_bytes!({index:?});\n\
             pub const APP_JS: &[u8] = include_bytes!({js_path:?});\n\
             pub const APP_WASM: &[u8] = include_bytes!({wasm_path:?});\n\
             pub const APP_BUILT: bool = {app_built};\n",
        ),
    )
    .expect("write generated.rs");
}

/// The real artifact when it exists, otherwise a placeholder file under
/// OUT_DIR so the generated `include_bytes!` compiles either way.
fn existing_or_placeholder(
    artifact: &std::path::Path,
    placeholder: &std::path::Path,
    contents: Vec<u8>,
) -> PathBuf {
    if artifact.is_file() {
        return artifact.to_path_buf();
    }
    fs::write(placeholder, contents).expect("write placeholder");
    placeholder.to_path_buf()
}

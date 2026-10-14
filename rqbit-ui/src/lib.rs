//! GPUI views and client abstraction shared by the rqbit desktop app and the
//! browser (wasm) client.
//!
//! This crate is librqbit-free and tokio-free: views render gpui-kit
//! components and fetch/mutate state exclusively through the
//! [`client::TorrentClient`] trait (in-process `Arc<librqbit::Api>` on
//! desktop, WebSocket protocol frames in the browser). Anything a plain view
//! cannot do — file dialogs, OS integration, the settings page — goes
//! through the [`host::UiHost`] callbacks provided by the embedding binary.

pub mod client;
pub mod host;
pub mod ui;

pub use client::{AddTorrentRequest, ClientResult, TorrentClient, UiAddSource};
pub use host::{PickedTorrentFile, UiCapabilities, UiHost};

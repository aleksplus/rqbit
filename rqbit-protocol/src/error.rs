//! Typed error carried inside `ServerMessage::Response` frames.

use serde::{Deserialize, Serialize};

/// Machine-readable error class. Mirrors the handful of `ApiError` cases the
/// UI actually distinguishes (e.g. DHT-disabled is signalled as "off" in the
/// footer, not surfaced as a failure).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtoErrorKind {
    /// The referenced torrent does not exist (or is gone after an action).
    TorrentNotFound,
    /// The torrent exists but its live state is unavailable.
    TorrentNotLive,
    /// DHT is turned off in the daemon's settings.
    DhtDisabled,
    /// The request was invalid (bad magnet, bad file indices, …).
    BadRequest,
    /// Anything else, with the message carrying the details.
    Internal,
}

/// A wire-safe error: an explicit kind plus a human-readable message.
///
/// Intentionally not `thiserror`/`anyhow`-shaped — this type crosses the wire
/// and must round-trip losslessly through postcard.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtoError {
    pub kind: ProtoErrorKind,
    pub message: String,
}

impl ProtoError {
    pub fn new(kind: ProtoErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ProtoErrorKind::Internal, message)
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(ProtoErrorKind::BadRequest, message)
    }
}

impl std::fmt::Display for ProtoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for ProtoError {}

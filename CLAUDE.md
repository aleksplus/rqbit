# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

rqbit is a BitTorrent client written in Rust and desktop app (GPUI). The library (`librqbit`) can also be used standalone.

## Build Commands

```bash
# Build (release)
cargo build --release

# Run tests
cargo test                    # default members only
cargo test --workspace        # all workspace members

# Run a specific test
cargo test <test_name>
cargo test -p librqbit <test_name>   # test in specific crate

# Lint
cargo fmt --all -- --check
cargo clippy --all-targets

```

### Log Files

Both devserver and testserver write DEBUG-level logs to files:
- **devserver**: `/tmp/rqbit-log` (env: `RQBIT_LOG_FILE`, `RQBIT_LOG_FILE_RUST_LOG`)
- **testserver**: `/tmp/rqbit-simulate-traffic/testserver.log` (env: `TESTSERVER_LOG_FILE`, `TESTSERVER_LOG_FILE_RUST_LOG`)

**IMPORTANT:** NEVER read log files directly - they can grow to many gigabytes. ALWAYS filter with `rg` and limit output:
```bash
rg "192.168.1.100" /tmp/rqbit-log | head -50      # search for peer address
rg "ERROR" /tmp/rqbit-log | tail -100             # recent errors
rg "abc123" /tmp/rqbit-log | head -20             # search for torrent hash
```

## Architecture

### Core Library (`crates/librqbit`)
The main library - the binary is just a thin CLI wrapper. Key components:

- **Session** (`session.rs`): Central coordinator managing torrents, DHT, peer connections, and persistence. Entry point for the library.
- **TorrentState** (`torrent_state/`): State machine for torrent lifecycle - initializing, live (downloading/seeding), paused
- **Storage** (`storage/`): Pluggable storage backends (filesystem, mmap) with middleware support (caching, timing)

### Supporting Crates
- `bencode` - Bencode serialization/deserialization
- `dht` - Distributed Hash Table (BEP-5)
- `peer_binary_protocol` - BitTorrent peer wire protocol
- `tracker_comms` - HTTP/UDP tracker communication
- `upnp` - Port forwarding
- `upnp-serve` - UPnP Media Server
- `librqbit_core` - Shared types (magnet links, torrent metainfo, peer IDs)
- `buffers` - Binary buffer utilities, small wrappers around bytes::Bytes and &[u8].
- `sha1w` - SHA1 wrapper (supports crypto-hash or openssl backends)


## Rust Development

### Warnings & Linting
- After making code changes in Rust, always run `cargo check` and `cargo clippy`
  before declaring work complete. Never claim compilation success without verifying.
- When fixing compiler warnings, batch all related warnings together and fix them
  in a single pass. Run `cargo check 2>&1` to capture the full list before editing.

## Desktop GPUI Client

The `desktop-gpui` crate provides a native desktop GUI client for rqbit using GPUI (Zed's UI framework). It uses librqbit::Api to communicate with the session, without HTTP endpoints or Prometheus.

### Building

```bash
# Build the desktop client
cargo build -p desktop-gpui

# Run the desktop client
cargo run -p desktop-gpui
```

### Architecture

- **librqbit::Api-only communication**: The desktop client communicates with the session via IPC, not HTTP.
- **GPUI framework**: Uses Zed's GPUI for a native, high-performance UI.
- **Shared state**: Configuration and session state are shared via `Arc<RwLock<SharedState>>`.

### Development

```bash
# Run with hot reload (if supported)
cargo run -p desktop-gpui

# Check code
cargo check -p desktop-gpui
cargo clippy -p desktop-gpui
```

## General Rules
- Never declare a task complete until tests actually pass and compilation is verified.
  If compilation cannot be verified due to pre-existing errors, explicitly state that caveat.
- When asked to create a plan or design document, present the plan for user review
  BEFORE starting implementation. Do not begin coding until the user explicitly approves.
- Prefer typed errors over anyhow for verification paths. This keeps error handling
  consistent and allows callers to distinguish recoverable from hard errors.

## Protocol & Spec Compliance
- When working on BEP 52 / torrent-related code, always use v2 (BEP 52) structures
  and info dicts, not v1. The spec explicitly requires BEP 52 compliance.

## Other directives
- Use narsil mcp for code retrieval or search (do not fallback to rg/grep or find if narsil provides response value)
- Use context7 for documentation
- Fetch https://longbridge.github.io/gpui-component/docs/ if needed.
- If you need to resort to running shell commands, always use "rg" instead of "grep".

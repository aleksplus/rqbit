# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

rqbit is a BitTorrent client written in Rust and desktop app (GPUI). The library (`librqbit`) can also be used standalone.

## Build Commands

```bash
# Build (release)
cargo build --release

# Build with gpui feature (requires npm installed)
cargo build --release --features gpui

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

## Development Server

```bash
# Run test server that simulates traffic. Points to http://localhost:3030 for the main session's web UI and API.
# If you make changes to Rust this needs to be restarted.
make testserver

# Run webui in dev mode (hot reload vite server). Points to http://localhost:3031.
# make webui-dev
```

@crates/librqbit/webui/CLAUDE.md has some details on webui if needed.

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
- If you need to resort to running shell commands, always use "rg" instead of "grep".
- Prefer using Serena MCP instead of searching / reading / writing raw files when makes sense.
- **Always run `npm run format` after modifying webui or desktop TypeScript/TSX files.**


# **Native GPUI RPC Interface**

The `desktop‑gpui` binary exposes a Unix‑domain socket IPC that implements the same JSON‑RPC contract as the HTTP API.  
All calls are authenticated with an HMAC token derived from a secret stored in `desktop‑gpui/secrets.toml`.  
The socket path is `/tmp/rqlib-ipc.sock` (only bound on localhost).

| # | Method | Description |
|---|--------|-------------|
| 1 | `create_magnet` | Create a new torrent entry. Returns the generated `tid`. |
| 2 | `get_state` | Query the internal status of an existing torrent (`live`, `seeding`, `paused`). |
| 3 | `set_peer_limit` | Set the maximum number of peers for a torrent. |
| 4 | `get_peer_list` | Retrieve a list of active peer addresses (IP/port, role). |
| 5 | `dht_peer_addr` | Query DHT nodes that are downloadable/seeding/trackers for a hash. |
| 6 | `stats` | Aggregate usage statistics (bytes downloaded, peers alive). |
| 7 | `get_session_config` | Read low‑level config stored in the SQLite session file. |
| 8 | `seed_magnet` | Start seeding a magnet from an external source directory. |
| 9 | `tracker_list` | List registered tracker URLs for a torrent. |
|10 | `delete_torrent` | Remove a torrent entry from the session database. |

## Request Payload

```json
{
  "cmd": "<method-name>",
  "payload": { … },          // method‑specific arguments
  "auth_token": "<hmac>"      // HMAC of the payload using secret from `secrets.toml`
}
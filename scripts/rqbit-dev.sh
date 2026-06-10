#!/bin/bash

export RQBIT_UPNP_SERVER_ENABLE="true"
export RQBIT_UPNP_SERVER_FRIENDLY_NAME="rqbit-dev"
export RQBIT_HTTP_API_LISTEN_ADDR="[::]:3030"
export RQBIT_ENABLE_PROMETHEUS_EXPORTER="true"
export RQBIT_EXPERIMENTAL_UTP_LISTEN_ENABLE="true"
export RQBIT_HTTP_API_ALLOW_CREATE="true"
export RQBIT_FASTRESUME="true"

# Setting Rust logger configuration and log file paths
export RQBIT_LOG_FILE="/tmp/rqbit-log"
export RQBIT_LOG_FILE_RUST_LOG="debug,librqbit=trace,upnp_serve=trace,librqbit_utp=debug"
export CORS_ALLOW_REGEXP=".*"

# Clearing the log file (similar to 'type nul > ...')
> "$RQBIT_LOG_FILE"

# Running the cargo command for starting the server
cargo run -- server start "$(realpath "$TEMP")"

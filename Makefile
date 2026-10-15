all:

.PHONY: webui-deps
webui-deps:
	cd desktop && npm install
	cd crates/librqbit/webui && npm install

.PHONY: webui-dev
webui-dev: webui-deps
	cd crates/librqbit/webui && \
	npm run dev

# NOTE: on LG TV using hostname is unstable for some reason, use IP address.
export RQBIT_UPNP_SERVER_ENABLE ?= true
export RQBIT_UPNP_SERVER_FRIENDLY_NAME ?= rqbit-dev
export RQBIT_HTTP_API_LISTEN_ADDR ?= [::]:3030
export RQBIT_ENABLE_PROMETHEUS_EXPORTER ?= true
export RQBIT_EXPERIMENTAL_UTP_LISTEN_ENABLE ?= true
export RQBIT_HTTP_API_ALLOW_CREATE ?= true
export RQBIT_FASTRESUME = true

CARGO_RUN_FLAGS ?=
RQBIT_OUTPUT_FOLDER ?= /tmp/scratch

PGUSER ?= $(shell whoami)
PGHOST ?= 127.0.0.1
PGPORT ?= 5432
RQBIT_POSTGRES_CONNECTION_STRING ?= postgres://${PGUSER}@${PGHOST}:${PGPORT}/rqbit

# Alternatively run this on OSX to profile easily
# cargo instruments --profile release-debug --features=_disable_disk_write_net_benchmark -t time --time-limit 20000 -- download -o /tmp/scratch/ --overwrite
.PHONY: devserver-profile
devserver-profile:
	cargo run -p rqbit --release $(CARGO_RUN_FLAGS) -- server start $(RQBIT_OUTPUT_FOLDER)

# DEV variables (that's why defined after devserver-profile)
export RQBIT_LOG_FILE ?= /tmp/rqbit-log
export RQBIT_LOG_FILE_RUST_LOG ?= debug,librqbit=trace,upnp_serve=trace,librqbit_utp=debug
export CORS_ALLOW_REGEXP ?= '.*'

.PHONY: devserver
devserver:
	echo -n '' > $(RQBIT_LOG_FILE) && \
	cargo run -p rqbit $(CARGO_RUN_FLAGS) -- \
	server start $(RQBIT_OUTPUT_FOLDER)

.PHONY: devserver-postgres
devserver-postgres:
	echo -n '' > $(RQBIT_LOG_FILE) && \
	cargo run -p rqbit $(CARGO_RUN_FLAGS) -- \
	server start --fastresume --persistence-location $(RQBIT_POSTGRES_CONNECTION_STRING) $(RQBIT_OUTPUT_FOLDER)

.PHONY: testserver
testserver:
	ulimit -n unlimited && cargo run -p librqbit --features http-api,tracing-subscriber-utils,webui,prometheus --example simulate_traffic

.PHONY: docker-build-xx-one-platform
docker-build-xx-one-platform:
	docker build -f docker/Dockerfile.xx \
		--platform $(PLATFORM) \
		--output type=local,dest=target/cross/$(PLATFORM) . && \
	docker build \
		-t ikatson/rqbit:$(shell git describe --tags)-dev-$(shell echo $(PLATFORM) | tr '/' '-') \
		--platform $(PLATFORM) \
		-f docker/Dockerfile \
		target/cross/

.PHONY: docker-build-amd64
docker-build-amd64:
	PLATFORM=linux/amd64 $(MAKE) docker-build-xx-one-platform

.PHONY: docker-build-armv7
docker-build-armv7:
		PLATFORM=linux/arm/v7 $(MAKE) docker-build-xx-one-platform

.PHONY: clean
clean:
	rm -rf target

CARGO_RELEASE_PROFILE ?= release-github

.PHONY: release-linux-current-target
release-linux-current-target:
	CC_$(TARGET_SNAKE_CASE)=$(CROSS_COMPILE_PREFIX)-gcc \
	CXX_$(TARGET_SNAKE_CASE)=$(CROSS_COMPILE_PREFIX)-g++ \
	AR_$(TARGET_SNAKE_CASE)=$(CROSS_COMPILE_PREFIX)-ar \
	CARGO_TARGET_$(TARGET_SNAKE_UPPER_CASE)_LINKER=$(CROSS_COMPILE_PREFIX)-gcc \
	cargo build  --profile $(CARGO_RELEASE_PROFILE) --target=$(TARGET) --features=openssl-vendored,prometheus

.PHONY: debug-linux-docker-x86_64
debug-linux-docker-x86_64:
	CARGO_RELEASE_PROFILE=dev \
	$(MAKE) release-linux-x86_64 && \
	cp target/x86_64-unknown-linux-musl/debug/rqbit target/cross/linux/amd64/ && \
	docker build -t ikatson/rqbit:tmp-debug -f docker/Dockerfile --platform linux/amd64 target/cross && \
	docker push ikatson/rqbit:tmp-debug

.PHONY: release-linux-x86_64
release-linux-x86_64:
	TARGET=x86_64-unknown-linux-musl \
	TARGET_SNAKE_CASE=x86_64_unknown_linux_musl \
	TARGET_SNAKE_UPPER_CASE=X86_64_UNKNOWN_LINUX_MUSL \
	CROSS_COMPILE_PREFIX=x86_64-unknown-linux-musl \
	$(MAKE) release-linux-current-target

.PHONY: release-linux-aarch64
release-linux-aarch64:
	TARGET=aarch64-unknown-linux-musl \
	TARGET_SNAKE_CASE=aarch64_unknown_linux_musl \
	TARGET_SNAKE_UPPER_CASE=AARCH64_UNKNOWN_LINUX_MUSL \
	CROSS_COMPILE_PREFIX=aarch64-unknown-linux-musl \
	$(MAKE) release-linux-current-target

.PHONY: release-linux-armv7
release-linux-armv7:
	TARGET=armv7-unknown-linux-musleabihf \
	TARGET_SNAKE_CASE=armv7_unknown_linux_musleabihf \
	TARGET_SNAKE_UPPER_CASE=ARMV7_UNKNOWN_LINUX_MUSLEABIHF \
	CROSS_COMPILE_PREFIX=armv7-linux-musleabihf \
	$(MAKE) release-linux-current-target

# --- web client (browser build of the GPUI client) ---
WASM_TARGET ?= wasm32-unknown-unknown
# Must match the `wasm-bindgen` crate version in Cargo.lock.
WASM_BINDGEN_VERSION ?= 0.2.127

# The GPUI web stack needs a nightly toolchain: gpui-kit's wasm path pulls
# gpui-pre-web with its default `multithreaded` feature, whose wasm_thread
# dependency uses `#![feature(...)]` (there is no opt-out - both gpui-kit and
# gpui-pre-platform enable it unconditionally). This matches gpui-kit's own
# documented WebAssembly pipeline. RUSTC must be pinned too: cargo resolves
# rustc via PATH, which would pick Homebrew's rustc (no wasm std in its
# sysroot) even when cargo itself is rustup's.
WASM_TOOLCHAIN ?= nightly
WASM_CARGO ?= $(shell rustup which --toolchain $(WASM_TOOLCHAIN) cargo 2>/dev/null || command -v cargo)
WASM_RUSTC ?= $(shell rustup which --toolchain $(WASM_TOOLCHAIN) rustc 2>/dev/null)
WASM_BINDGEN ?= $(shell command -v wasm-bindgen 2>/dev/null || echo $(HOME)/.cargo/bin/wasm-bindgen)

# One-time setup (Homebrew rust has no rustup/wasm std by default):
#   brew install rustup && \
#   rustup toolchain install $(WASM_TOOLCHAIN) --profile minimal --target $(WASM_TARGET) && \
#   rustup run $(WASM_TOOLCHAIN) cargo install wasm-bindgen-cli --version $(WASM_BINDGEN_VERSION)

# Build the wasm payload. app-daemon embeds it at compile time, so after this
# run `make daemon` and open http://localhost:3080/ — the client is served by
# the daemon itself, and the WebSocket protocol lives at /ws on the same port.
.PHONY: web-client-build
web-client-build:
# Release by default: the debug payload is ~140 MB and slow to instantiate
# in the browser; release is ~29 MB. Pass DEBUG=1 for fast iteration builds.
	RUSTC=$(WASM_RUSTC) $(WASM_CARGO) build -p web-gpui --target $(WASM_TARGET) $(if $(DEBUG),,--release) && \
	$(WASM_BINDGEN) --target web \
		--out-dir web-gpui/dist/pkg \
		target/$(WASM_TARGET)/$(if $(DEBUG),debug,release)/web_gpui.wasm

# Headless core daemon the UI clients connect to (ws://localhost:3080/ws).
# Also serves the web client at /.
# NOTE: the desktop app's embedded HTTP API defaults to 127.0.0.1:3030 too -
# disable it there (or quit the app) before running both.
.PHONY: daemon
daemon:
	cargo run -p app-daemon

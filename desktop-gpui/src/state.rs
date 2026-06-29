use crate::config::RqbitDesktopConfig;
use librqbit::{api::Api, tracing_subscriber_config_utils::InitLoggingResult};
use librqbit_dualstack_sockets::TcpListener;
use std::{
    fs::{File, OpenOptions},
    io::{BufReader, BufWriter},
    path::Path,
    sync::Arc,
};

use parking_lot::RwLock;

use anyhow::Context;
use http::StatusCode;
use librqbit::{
    ApiError, DhtSessionConfig, Session, SessionOptions, SessionPersistenceConfig, WithStatusError,
    dht::DhtPersistenceConfig,
};
use tracing::{debug_span, error, warn};

#[derive(Clone)]
pub struct StateShared {
    config: RqbitDesktopConfig,
    api: Arc<Api>,
}

pub struct State {
    config_filename: String,
    shared: Arc<RwLock<SharedState>>,
    init_logging: InitLoggingResult,
}

fn read_config(path: &str) -> anyhow::Result<RqbitDesktopConfig> {
    let rdr = BufReader::new(File::open(path)?);
    let mut config: RqbitDesktopConfig = serde_json::from_reader(rdr)?;
    config.persistence.fix_backwards_compat();
    Ok(config)
}

fn write_config(path: &str, config: &RqbitDesktopConfig) -> anyhow::Result<()> {
    std::fs::create_dir_all(Path::new(path).parent().context("no parent")?)
        .context("error creating dirs")?;
    let tmp = format!("{}.tmp", path);
    let mut tmp_file = BufWriter::new(
        OpenOptions::new()
            .write(true)
            .truncate(true)
            .create(true)
            .open(&tmp)?,
    );
    serde_json::to_writer(&mut tmp_file, config)?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

async fn api_from_config(
    init_logging: &InitLoggingResult,
    config: &RqbitDesktopConfig,
) -> anyhow::Result<Api> {
    config
        .validate()
        .context("error validating configuration")?;
    let persistence = if config.persistence.disable {
        None
    } else {
        Some(SessionPersistenceConfig::Json {
            folder: if config.persistence.folder == Path::new("") {
                None
            } else {
                Some(config.persistence.folder.clone())
            },
        })
    };

    let (listen, connect) = config.connections.as_listener_and_connect_opts();

    let mut http_api_opts = librqbit::http_api::HttpApiOptions {
        read_only: config.http_api.read_only,
        basic_auth: None,
        ..Default::default()
    };

    // We need to start prometheus recorder earlier than session.
    if !config.http_api.disable {
        match metrics_exporter_prometheus::PrometheusBuilder::new().install_recorder() {
            Ok(handle) => {
                http_api_opts.prometheus_handle = Some(handle);
            }
            Err(e) => {
                warn!("error installting prometheus recorder: {e:#}");
            }
        }
    }

    let dht = if config.dht.disable {
        None
    } else {
        let persistence = if config.dht.disable_persistence {
            None
        } else {
            Some(DhtPersistenceConfig {
                config_filename: Some(config.dht.persistence_filename.clone()),
                ..Default::default()
            })
        };
        Some(DhtSessionConfig {
            persistence,
            ..Default::default()
        })
    };

    let session = Session::new_with_opts(
        config.default_download_location.clone(),
        SessionOptions {
            dht,
            persistence,
            connect: Some(connect),
            listen,
            fastresume: config.persistence.fastresume,
            ratelimits: config.ratelimits,
            #[cfg(feature = "disable-upload")]
            disable_upload: config.disable_upload,
            ..Default::default()
        },
    )
    .await
    .context("couldn't set up librqbit session")?;

    let api = Api::new(
        session.clone(),
        Some(init_logging.rust_log_reload_tx.clone()),
        Some(init_logging.line_broadcast.clone()),
    );

    if !config.http_api.disable {
        let listen_addr = config.http_api.listen_addr;
        let api = api.clone();
        let upnp_router = if config.upnp.enable_server {
            let friendly_name = config
                .upnp
                .server_friendly_name
                .as_ref()
                .map(|f| f.trim())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_owned())
                .unwrap_or_else(|| {
                    format!(
                        "rqbit-desktop@{}",
                        gethostname::gethostname().to_string_lossy()
                    )
                });

            let mut upnp_adapter = session
                .make_upnp_adapter(friendly_name, config.http_api.listen_addr.port())
                .await
                .context("error starting UPnP server")?;
            let router = upnp_adapter.take_router()?;
            session.spawn(debug_span!("ssdp"), "ssdp", async move {
                upnp_adapter.run_ssdp_forever().await
            });
            Some(router)
        } else {
            None
        };
        let http_api_task = async move {
            let listener = TcpListener::bind_tcp(listen_addr, Default::default())
                .with_context(|| format!("error listening on {}", listen_addr))?;
            librqbit::http_api::HttpApi::new(api.clone(), Some(http_api_opts))
                .make_http_api_and_run(listener, upnp_router)
                .await
        };

        session.spawn(debug_span!("http_api"), "http_api", http_api_task);
    }
    Ok(api)
}

impl State {
    pub async fn new(init_logging: InitLoggingResult) -> Self {
        let config_filename = directories::ProjectDirs::from("com", "rqbit", "desktop")
            .expect("directories::ProjectDirs::from")
            .config_dir()
            .join("config.json")
            .to_str()
            .expect("to_str()")
            .to_owned();

        let config = if let Ok(cfg) = read_config(&config_filename) {
            cfg
        } else {
            RqbitDesktopConfig::default()
        };

        let api = api_from_config(&init_logging, &config)
            .await
            .map_err(|e| {
                warn!(error=?e, "error reading configuration");
                e
            })
            .context("error creating state")?;

        let shared = Arc::new(RwLock::new(StateShared { config, api }));

        Self {
            config_filename,
            shared,
            init_logging,
        }
    }

    pub fn api(&self) -> Arc<Api> {
        self.shared.read().api.clone()
    }

    async fn configure(&self, config: RqbitDesktopConfig) -> Result<(), ApiError> {
        {
            let g = self.shared.read();
            if g.config == config {
                // The config didn't change, and the API is running, nothing to do.
                return Ok(());
            }
        }

        let existing = self.shared.write().as_mut();

        existing.api.session().stop().await;

        let api = api_from_config(&self.init_logging, &config).await?;
        if let Err(e) = write_config(&self.config_filename, &config) {
            error!("error writing config: {:#}", e);
        }

        existing.config = config;
        existing.api = Arc::new(api);
        Ok(())
    }
}

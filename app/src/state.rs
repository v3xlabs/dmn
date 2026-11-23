use crate::{
    cache::AppCache,
    database::Database,
    modules::{
        cloudflare::CloudflareService, ens::EnsService, manual::ManualService, ntfy::NtfyService,
        porkbun::PorkbunService,
    },
    Error,
};
use anyhow::Context;
use dirs;
use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};
use tracing::{info, warn};

pub type AppState = Arc<AppStateInner>;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DatabaseConfig {
    pub url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServerConfig {
    pub secret: String,
    #[serde(default = "default_listen")]
    pub listen: String,
}

fn default_listen() -> String {
    "0.0.0.0:3000".to_string()
}

pub struct AppStateInner {
    pub database: Database,
    pub api: Option<ServerConfig>,
    pub cache: AppCache,
    pub porkbun: Option<PorkbunService>,
    pub cloudflare: Option<CloudflareService>,
    pub ens: Option<EnsService>,
    pub manual: Option<ManualService>,
    pub ntfy: Option<NtfyService>,
    /// The config the services above were built from.
    pub config: LoadedConfig,
}

pub struct LoadedConfig {
    pub figment: Figment,
    pub path: PathBuf,
    pub deletes: Option<DeletesAllowed>,
}

/// Proof for `Domain::delete_by_name` that the config file existed at startup; `index_state` and the zone cache follow `domains` and the API, so they need none.
pub struct DeletesAllowed(());

/// Reads `DMN_CONFIG_PATH` or the default path, and writes the default config there when it is absent and `create_if_missing` is set.
///
/// # Errors
///
/// When the path exists but cannot be read or is not valid TOML, or when the default config cannot be created.
pub fn get_config_file(create_if_missing: bool) -> Result<LoadedConfig, Error> {
    let path = std::env::var("DMN_CONFIG_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs::config_dir().unwrap().join("dmn").join("config.toml"));

    // `symlink_metadata`, so a dangling symlink is an unreadable file and not a missing one.
    match std::fs::symlink_metadata(&path) {
        Ok(_) => info!("Using config file at {}", path.display()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            if !create_if_missing {
                return Ok(LoadedConfig {
                    figment: Figment::new(),
                    path,
                    deletes: None,
                });
            }
            if let Some(dir) = path.parent().filter(|p| !p.exists()) {
                info!("Creating default config directory at {}", dir.display());
                std::fs::create_dir_all(dir)
                    .with_context(|| format!("cannot create config directory {}", dir.display()))?;
            }
            info!("Creating default config file at {}", path.display());
            std::fs::write(&path, include_str!("../config.toml"))
                .with_context(|| format!("cannot create config file {}", path.display()))?;
            return Ok(LoadedConfig {
                figment: Figment::from(Toml::string(include_str!("../config.toml"))),
                path,
                deletes: None,
            });
        }
        Err(err) => {
            return Err(
                Error::new(err).context(format!("cannot read config file {}", path.display()))
            )
        }
    }

    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("cannot read config file {}", path.display()))?;
    let figment = Figment::from(Toml::string(&text));
    figment
        .extract::<figment::value::Dict>()
        .with_context(|| format!("config file {} is not valid TOML", path.display()))?;
    Ok(LoadedConfig {
        figment,
        path,
        deletes: Some(DeletesAllowed(())),
    })
}

impl AppStateInner {
    pub async fn init(server: bool) -> Result<Self, Error> {
        let config = get_config_file(server)?;
        if config.deletes.is_none() {
            warn!(
                config.path = %config.path.display(),
                "config file did not exist at startup, syncs add and update rows but delete none"
            );
        }
        let config_file = &config.figment;

        // Determine database URL: prefer DMN_DATABASE_URL, else default
        let database_config = Figment::new()
            .merge(Env::prefixed("DMN_DATABASE_"))
            .extract::<DatabaseConfig>()
            .expect("Failed to load database config");
        let database = Database::init(&database_config).await;

        let api = match Figment::new()
            .merge(Env::prefixed("DMN_API_"))
            .extract::<Option<ServerConfig>>()
        {
            Ok(Some(api)) => Some(api),
            Ok(None) => {
                if server {
                    panic!("Failed to load API config");
                } else {
                    None
                }
            }
            Err(error) => {
                if server {
                    panic!("Failed to load API config: {}", error);
                } else {
                    None
                }
            }
        };

        let cache = AppCache::new();

        let porkbun = if server {
            PorkbunService::try_init(config_file).await
        } else {
            None
        };
        let cloudflare = if server {
            CloudflareService::try_init(config_file).await
        } else {
            None
        };
        let ens = if server {
            EnsService::try_init(config_file).await
        } else {
            None
        };
        let manual = if server {
            ManualService::from_config(config_file)
        } else {
            None
        };

        let ntfy = NtfyService::try_init(config_file).await;

        Ok(Self {
            database,
            cache,
            api,
            porkbun,
            cloudflare,
            ens,
            manual,
            ntfy,
            config,
        })
    }
}

impl std::fmt::Debug for AppStateInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppStateInner").finish()
    }
}

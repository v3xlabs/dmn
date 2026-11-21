use crate::{
    cache::AppCache,
    database::Database,
    modules::{
        cloudflare::CloudflareService, ens::EnsService, ntfy::NtfyService, porkbun::PorkbunService,
    },
};
use dirs;
use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};
use tracing::{error, info};

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
    pub ntfy: Option<NtfyService>,
}

pub fn get_config_file(create_if_missing: bool) -> Option<Figment> {
    let config_file = std::env::var("DMN_CONFIG_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs::config_dir().unwrap().join("dmn").join("config.toml"));

    if !config_file.exists() {
        if !create_if_missing {
            return None;
        }
        if let Some(dir) = config_file.parent().filter(|p| !p.exists()) {
            info!("Creating default config directory at {}", dir.display());
            if let Err(e) = std::fs::create_dir_all(dir) {
                error!("Failed to create config directory: {}", e);
                return None;
            }
        }
        info!("Creating default config file at {}", config_file.display());
        if let Err(e) = std::fs::write(&config_file, include_str!("../config.toml")) {
            error!("Failed to create config file: {}", e);
            return None;
        }
    } else {
        info!("Using config file at {}", config_file.display());
    }

    Some(Figment::new().merge(Toml::file(config_file)))
}

impl AppStateInner {
    pub async fn init(server: bool) -> Self {
        let config_file = get_config_file(server).unwrap_or_default();

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
            PorkbunService::try_init(&config_file).await
        } else {
            None
        };
        let cloudflare = if server {
            CloudflareService::try_init(&config_file).await
        } else {
            None
        };
        let ens = if server {
            EnsService::try_init(&config_file).await
        } else {
            None
        };

        let ntfy = NtfyService::try_init(&config_file).await;

        Self {
            database,
            cache,
            api,
            porkbun,
            cloudflare,
            ens,
            ntfy,
        }
    }
}

impl std::fmt::Debug for AppStateInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppStateInner").finish()
    }
}

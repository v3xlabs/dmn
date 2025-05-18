use crate::{
    cache::AppCache,
    database::Database,
    modules::{cloudflare::CloudflareService, ntfy::NtfyService, porkbun::PorkbunService},
};
use dirs;
use figment::{
    providers::{Env, Format, Toml},
    Figment,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info};

pub type AppState = Arc<AppStateInner>;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DatabaseConfig {
    pub url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ServerConfig {
    pub secret: String,
}

pub struct AppStateInner {
    pub database: Database,
    pub api: Option<ServerConfig>,
    pub cache: AppCache,
    pub porkbun: Option<PorkbunService>,
    pub cloudflare: Option<CloudflareService>,
    pub ntfy: Option<NtfyService>,
}

fn get_config_file(server: bool) -> Option<Figment> {
    if !server {
        return None;
    }

    // read from `~/.config/dmn/config.toml`
    let config_dir = dirs::config_dir().unwrap();
    let config_dmn_dir = config_dir.join("dmn");

    if !config_dmn_dir.exists() {
        info!(
            "Creating default config directory at {}",
            config_dmn_dir.display()
        );
        let x = std::fs::create_dir_all(&config_dmn_dir);

        if let Err(e) = x {
            error!("Failed to create config directory: {}", e);
            return None;
        }
    }

    let config_file = config_dmn_dir.join("config.toml");

    // if file doesnt exist create it by copying from hardcoded file `../config.toml`
    if !config_file.exists() {
        info!("Creating default config file at {}", config_file.display());
        let default_config = include_str!("../config.toml");
        let x = std::fs::write(&config_file, default_config);

        if let Err(e) = x {
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
        // Load configuration from environment variables
        let config_file = match get_config_file(server) {
            Some(config_file) => config_file,
            None => {
                error!("Failed to load config file");
                Figment::new()
            }
        };

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

        let ntfy = NtfyService::try_init(&config_file).await;

        Self {
            database,
            cache,
            api,
            porkbun,
            cloudflare,
            ntfy,
        }
    }
}

impl std::fmt::Debug for AppStateInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppStateInner").finish()
    }
}

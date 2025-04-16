use crate::{
    cache::AppCache,
    database::Database,
    modules::{cloudflare::CloudflareService, porkbun::PorkbunService},
};
use dirs;
use figment::{providers::Env, Figment};
use serde::{Deserialize, Serialize};
use shellexpand;
use std::env;
use std::path::PathBuf;
use std::sync::Arc;

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
}

impl AppStateInner {
    pub async fn init(optimistic: bool) -> Self {
        // Load configuration from environment variables
        let config_file = Figment::new();

        // Determine database URL: prefer DATABASE_URL, else default
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
                if optimistic {
                    panic!("Failed to load API config");
                } else {
                    None
                }
            }
            Err(error) => {
                if optimistic {
                    panic!("Failed to load API config: {}", error);
                } else {
                    None
                }
            }
        };

        let cache = AppCache::new();

        let porkbun = if optimistic {
            PorkbunService::try_init(&config_file).await
        } else {
            None
        };
        let cloudflare = if optimistic {
            CloudflareService::try_init(&config_file).await
        } else {
            None
        };

        Self {
            database,
            cache,
            api,
            porkbun,
            cloudflare,
        }
    }
}

impl std::fmt::Debug for AppStateInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppStateInner").finish()
    }
}

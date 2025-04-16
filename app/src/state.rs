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
        let database_url = if let Ok(url) = env::var("DATABASE_URL") {
            url
        } else {
            panic!("DATABASE_URL is not set");
        };
        let database_config = DatabaseConfig {
            url: Some(database_url),
        };
        let database = Database::init(&database_config).await;

        let api = Figment::new()
            .merge(Env::prefixed("DMN_API_"))
            .extract::<Option<ServerConfig>>()
            .expect("Failed to load API secret");

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

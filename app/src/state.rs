use crate::{cache::AppCache, database::Database, modules::{cloudflare::CloudflareService, porkbun::PorkbunService}};
use figment::{providers::Env, Figment};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

pub type AppState = Arc<AppStateInner>;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DatabaseConfig {
    pub url: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct JwtConfig {
    pub secret: String,
}

pub struct AppStateInner {
    pub database: Database,
    pub jwt: JwtConfig,
    pub cache: AppCache,
    pub porkbun: Option<PorkbunService>,
    pub cloudflare: Option<CloudflareService>,
}

impl AppStateInner {
    pub async fn init() -> Self {
        // Load configuration from environment variables
        let config_file = Figment::new();

        let database_config = Figment::new()
            .merge(Env::prefixed("DATABASE_"))
            .extract::<DatabaseConfig>()
            .expect("Failed to load database configuration");

        let database = Database::init(&database_config).await;

        let jwt = Figment::new()
            .merge(Env::prefixed("JWT_"))
            .extract::<JwtConfig>()
            .expect("Failed to load JWT secret");

        let cache = AppCache::new();

        let porkbun = PorkbunService::try_init(&config_file).await;
        let cloudflare = CloudflareService::try_init(&config_file).await;

        Self {
            database,
            cache,
            jwt,
            porkbun,
            cloudflare,
        }
    }
}

impl std::fmt::Debug for AppStateInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppStateInner")
            .finish()
    }
}

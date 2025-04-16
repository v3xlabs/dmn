use crate::{cache::AppCache, database::Database};
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

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PorkbunConfig {
    pub api_key: Option<String>,
}

pub struct AppStateInner {
    pub database: Database,
    pub porkbun_config: PorkbunConfig,
    pub jwt: JwtConfig,
    pub cache: AppCache,
}

impl AppStateInner {
    pub async fn init() -> Self {
        // Load configuration from environment variables
        let porkbun_config = Figment::new()
            .merge(Env::prefixed("PORKBUN_"))
            .extract::<PorkbunConfig>()
            .expect("Failed to load Porkbun configuration");

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

        Self {
            database,
            porkbun_config,
            jwt,
            cache,
        }
    }
}

impl std::fmt::Debug for AppStateInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppStateInner")
            .finish()
    }
}

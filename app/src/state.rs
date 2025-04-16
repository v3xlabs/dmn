use crate::{cache::AppCache, database::Database, modules::{cloudflare::CloudflareService, porkbun::PorkbunService}};
use figment::{providers::Env, Figment};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::env;
use std::path::PathBuf;
use dirs;
use shellexpand;

pub type AppState = Arc<AppStateInner>;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DatabaseConfig {
    pub url: Option<String>,
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

        // Determine database URL: prefer DATABASE_URL, else DMN_DB_PATH, else default
        let database_url = if let Ok(url) = env::var("DATABASE_URL") {
            url
        } else {
            let db_path = env::var("DMN_DB_PATH").unwrap_or_else(|_| {
                let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
                format!("{}/.config/dmn/sqlite", home.display())
            });
            // Ensure parent directory exists
            let db_path = shellexpand::tilde(&db_path).to_string();
            std::fs::create_dir_all(PathBuf::from(&db_path).parent().unwrap()).ok();
            format!("sqlite://{}", db_path)
        };
        let database_config = DatabaseConfig { url: Some(database_url) };
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

use cloudflare::framework::{auth::Credentials, client::async_api::Client};
use figment::{providers::Env, Figment};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

pub mod dns;
pub mod domains;
pub mod domains_endpoint;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CloudflareConfig {
    pub api_key: Option<String>,
    pub global_api_key: Option<String>,
    pub email: Option<String>,
}

pub struct CloudflareService {
    pub config: CloudflareConfig,
    pub client: Client,
}

impl CloudflareConfig {
    fn credentials(&self) -> Option<Credentials> {
        let api_key = self
            .api_key
            .as_deref()
            .filter(|value| !value.trim().is_empty());
        let global_api_key = self
            .global_api_key
            .as_deref()
            .filter(|value| !value.trim().is_empty());
        let email = self
            .email
            .as_deref()
            .filter(|value| !value.trim().is_empty());

        match (global_api_key, email, api_key) {
            (Some(key), Some(email), _) => Some(Credentials::UserAuthKey {
                key: key.to_owned(),
                email: email.to_owned(),
            }),
            (Some(_), None, _) => {
                warn!("Cloudflare global_api_key is set without email, Cloudflare disabled");
                None
            }
            (None, _, Some(token)) => Some(Credentials::UserAuthToken {
                token: token.to_owned(),
            }),
            (None, _, None) => {
                info!("Cloudflare credentials not set, Cloudflare disabled");
                None
            }
        }
    }
}

impl CloudflareService {
    pub fn new(config: CloudflareConfig) -> Option<Self> {
        let credentials = config.credentials()?;

        let cf_config = cloudflare::framework::client::ClientConfig::default();
        let cf_env = cloudflare::framework::Environment::Production;
        let client = Client::new(credentials, cf_config, cf_env).unwrap();

        Some(Self { config, client })
    }

    pub async fn try_init(provider: &impl figment::Provider) -> Option<Self> {
        let config = Figment::new()
            .merge(Env::prefixed("CLOUDFLARE_"))
            .merge(provider)
            .extract::<CloudflareConfig>();
        if let Ok(config) = config {
            let service = Self::new(config)?;
            info!("Cloudflare config verified");
            if let Err(error) = service.fetch_zone_page(1).await {
                warn!("Cloudflare token check failed: {error:?}");
                return None;
            }
            info!("Cloudflare token valid (get zones successful)");
            Some(service)
        } else {
            warn!("Cloudflare config verification failed");
            None
        }
    }
}

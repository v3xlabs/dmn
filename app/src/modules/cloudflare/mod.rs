use cloudflare::{
    endpoints::zones::zone::{ListZones, ListZonesParams, Zone},
    framework::{auth::Credentials, client::async_api::Client, response::ApiFailure},
};
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

impl CloudflareService {
    pub fn new(config: CloudflareConfig) -> Self {
        let credentials = if let Some(global_api_key) = &config.global_api_key {
            Credentials::UserAuthKey {
                key: global_api_key.clone(),
                email: config.email.clone().unwrap_or_else(|| {
                    panic!("No email provided");
                }),
            }
        } else if let Some(api_key) = &config.api_key {
            Credentials::UserAuthToken { token: api_key.clone() }
        } else {
            panic!("No API key provided");
        };

        let cf_config = cloudflare::framework::client::ClientConfig::default();
        let cf_env = cloudflare::framework::Environment::Production;
        let client = Client::new(credentials, cf_config, cf_env).unwrap();

        Self { config, client }
    }

    pub async fn try_init(provider: &impl figment::Provider) -> Option<Self> {
        let config = Figment::new()
            .merge(Env::prefixed("CLOUDFLARE_"))
            .merge(provider)
            .extract::<CloudflareConfig>();
        if let Ok(config) = config {
            let service = Self::new(config);
            info!("Cloudflare config verified");
            service.get_zones().await.ok()?;
            info!("Cloudflare token valid (get zones successful)");
            Some(service)
        } else {
            warn!("Cloudflare config verification failed");
            None
        }
    }

    pub async fn get_zones(&self) -> Result<Vec<Zone>, ApiFailure> {
        let list_zones = ListZones {
            params: ListZonesParams::default(),
        };

        let zones = self.client.request(&list_zones).await?;

        let peek = zones.result.len();
        info!("Cloudflare zones: {:?}", peek);

        Ok(zones.result)
    }
}

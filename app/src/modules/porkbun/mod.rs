use anyhow::Error;
use figment::{providers::Env, Figment};
use serde::{Deserialize, Serialize};
use reqwest;
use tracing::{info, warn};

pub mod domains;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PorkbunConfig {
    pub api_key: Option<String>,
    pub secret_key: Option<String>,
}

pub struct PorkbunService {
    config: PorkbunConfig,
}

#[derive(Serialize)]
pub struct PingRequest<'a> {
    pub apikey: &'a str,
    pub secretapikey: &'a str,
}

#[derive(Deserialize, Debug)]
pub struct PingResponse {
    pub status: String,
    pub yourIp: Option<String>,
}

impl PorkbunService {
    pub fn new(config: PorkbunConfig) -> Self {
        Self { config }
    }

    pub async fn try_init(provider: &impl figment::Provider) -> Option<Self> {
        let config = Figment::new().merge(Env::prefixed("PORKBUN_")).merge(provider).extract::<PorkbunConfig>();
        if let Ok(config) = config {
            let service = Self::new(config);
            info!("Porkbun config verified");
            service.ping().await.ok()?;
            info!("Porkbun token valid (ping successful)");
            Some(service)
        } else {
            warn!("Porkbun config verification failed");
            None
        }
    }

    pub async fn ping(&self) -> Result<String, Error> {
        let api_key = self.config.api_key.as_ref().ok_or_else(|| anyhow::anyhow!("Missing api_key"))?;
        let secret_key = self.config.secret_key.as_ref().ok_or_else(|| anyhow::anyhow!("Missing secret_key"))?;
        let client = reqwest::Client::new();
        let req_body = PingRequest {
            apikey: api_key,
            secretapikey: secret_key,
        };
        let response = client
            .post("https://api.porkbun.com/api/json/v3/ping")
            .json(&req_body)
            .send()
            .await?;
        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            return Err(anyhow::anyhow!("Ping failed: {}", text));
        }
        let resp: PingResponse = serde_json::from_str(&text)?;
        if resp.status != "SUCCESS" {
            return Err(anyhow::anyhow!("Ping error: {}", resp.status));
        }
        Ok(resp.yourIp.unwrap_or_default())
    }
}

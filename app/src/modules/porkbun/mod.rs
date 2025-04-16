use crate::state::PorkbunConfig;
use anyhow::Error;
use serde::{Deserialize, Serialize};
use reqwest;

pub mod domains;

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

    /**
     * Authentication
Authentication is done by passing an API key and secret API key in the JSON content posted to the URI endpoint. All API calls must include valid API keys. You can create API keys at porkbun.com/account/api. You can test communication with the API using the ping endpoint. The ping endpoint will also return your IP address, this can be handy when building dynamic DNS clients.

Get API Key

Example
URI Endpoint: https://api.porkbun.com/api/json/v3/ping


JSON Command Example

{
	"secretapikey": "YOUR_SECRET_API_KEY",
	"apikey": "YOUR_API_KEY"
}

JSON Response Example

{
	"status": "SUCCESS",
	"yourIp": "77.162.232.110"
}
     */

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

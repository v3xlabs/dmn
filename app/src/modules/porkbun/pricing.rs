use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{models::domain_tld_price::DomainTLDPrice, state::AppState, Error};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct PorkbunPricingConfig {
    pub enabled: bool,
    pub interval: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PorkbunTLDPrice {
    // "1.34"
    pub registration: String,
    pub renewal: String,
    pub transfer: String,
    pub coupons: Vec<String>,
    #[serde(rename = "specialType")]
    pub special_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct PorkbunTLDPriceResponse {
    pub status: String,
    pub pricing: HashMap<String, PorkbunTLDPrice>,
}

pub async fn ingest_domain_tld_prices(state: &AppState) -> Result<(), Error> {
    // TODO: Implement
    // get request to https://api.porkbun.com/api/json/v3/pricing/get
    // long lasting (takes up to 30seconds)
    // rustls
    info!("Ingesting domain tld prices");
    let client = reqwest::Client::builder().use_rustls_tls().build()?;

    let response = client
        .get("https://api.porkbun.com/api/json/v3/pricing/get")
        .send()
        .await?;

    let body = response.text().await?;

    let porkbun_tld_price_response: PorkbunTLDPriceResponse = serde_json::from_str(&body)?;

    info!("Porkbun tld prices: {:?}", porkbun_tld_price_response);

    for (tld, price) in porkbun_tld_price_response.pricing.iter() {
        println!("{}: {:?}", tld, price);
        let price = price.registration.parse::<f64>()?;
        // price currently in dollars
        let price_rounded = (price * 100.0).round() as i64;
        let _domain_tld_price = DomainTLDPrice::new(
            "porkbun".to_string(),
            tld.to_string(),
            price_rounded,
            state,
        ).await?;
    }

    Ok(())
}

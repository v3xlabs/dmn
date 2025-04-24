use super::{pricing::ingest_domain_tld_prices, PorkbunService};
use crate::{models::domain::Domain, modules::DomainService, state::AppState, util::serde_strint::string_or_int_to_option_i32};
use anyhow::Error;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::info;
use chrono::NaiveDateTime;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PorkbunDomain {
    pub domain: String,
    pub status: Option<String>,
    pub tld: Option<String>,
    pub create_date: Option<String>,
    pub expire_date: Option<String>,
    #[serde(deserialize_with = "string_or_int_to_option_i32")]
    pub security_lock: Option<i32>,
    #[serde(deserialize_with = "string_or_int_to_option_i32")]
    pub whois_privacy: Option<i32>,
    #[serde(deserialize_with = "string_or_int_to_option_i32")]
    pub auto_renew: Option<i32>,
    #[serde(deserialize_with = "string_or_int_to_option_i32")]
    pub not_local: Option<i32>,
    // Add more fields as needed from the API
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PorkbunDomainData {
    pub status: String,
    pub domains: Vec<PorkbunDomain>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ListAllRequest<'a> {
    apikey: &'a str,
    secretapikey: &'a str,
    start: Option<i32>,
    // 'yes'
    includeLabels: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ListAllResponse {
    status: String,
    domains: Vec<PorkbunDomain>,
}

/// Based on https://porkbun.com/api/json/v3/documentation#
impl DomainService for PorkbunService {
    async fn ingest_domains(&self, state: &AppState) -> Result<Vec<Domain>, Error> {
        let api_key = self
            .config
            .api_key
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing api_key"))?;
        let secret_key = self
            .config
            .secret_key
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Missing secret_key"))?;
        let client = reqwest::Client::new();
        let req_body = ListAllRequest {
            apikey: api_key,
            secretapikey: secret_key,
            start: Some(0),
            includeLabels: None,
        };
        let response = client
            .post("https://api.porkbun.com/api/json/v3/domain/listAll")
            .json(&req_body)
            .send()
            .await?;
        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            return Err(anyhow::anyhow!("Domain listAll failed: {}", text));
        }
        let resp: ListAllResponse = serde_json::from_str(&text)?;
        if resp.status != "SUCCESS" {
            return Err(anyhow::anyhow!("Domain listAll error: {}", resp.status));
        }

        let mut result_domains: Vec<Domain> = Vec::new();

        for domain in resp.domains {
            let metadata = json!({
                "status": domain.status,
                "tld": domain.tld,
                "create_date": domain.create_date,
                "expire_date": domain.expire_date,
                "security_lock": domain.security_lock,
            });
            let ext_expiry_at = if let Some(expire_date) = domain.expire_date {
                match NaiveDateTime::parse_from_str(&expire_date, "%Y-%m-%d %H:%M:%S") 
                    .map(|naive| naive.and_utc()) {
                        Ok(x) => Some(x),
                        Err(errors) => {
                            tracing::error!("Error parsing expiry date: {}", errors);
                            None
                        }
                    }
            } else {
                None
            };
            let ext_registered_at = if let Some(create_date) = domain.create_date {
                match NaiveDateTime::parse_from_str(&create_date, "%Y-%m-%d %H:%M:%S") 
                    .map(|naive| naive.and_utc()) {
                        Ok(x) => Some(x),
                        Err(errors) => {
                            tracing::error!("Error parsing expiry date: {}", errors);
                            None
                        }
                    }
            } else {
                None
            };

            let ext_auto_renew = domain.auto_renew.map(|x| x == 1);
            let ext_whois_privacy = domain.whois_privacy.map(|x| x == 1);

            let domain = Domain::new(
                domain.domain.clone(),
                "porkbun".to_string(),
                domain.domain.clone(),
                ext_expiry_at,
                ext_registered_at,
                ext_auto_renew,
                ext_whois_privacy,
                Some(metadata),
                &state,
            )
            .await
            .unwrap();

            info!("Porkbun domain ingested: {:?}", domain);

            result_domains.push(domain);
        }

        info!("Completed porkbun");

        Ok(result_domains)
    }

    async fn ingest_domain_tld_prices(&self, state: &AppState) -> Result<(), Error> {
        ingest_domain_tld_prices(state).await
    }
}

#[cfg(test)]
mod tests {

    use std::sync::Arc;

    use serde_json::json;

    use crate::{models::domain::Domain, state::AppStateInner};

    use super::*;

    #[async_std::test]
    async fn test_get_domains() {
        dotenvy::dotenv().ok();
        let state = Arc::new(AppStateInner::init(true).await);

        let domains = state.porkbun.as_ref().unwrap().ingest_domains(&state).await.unwrap();
    }
}

use std::collections::HashSet;

use super::{pricing::ingest_domain_tld_prices, PorkbunService};
use crate::{
    models::domain::Domain,
    modules::{DomainService, Synced},
    state::AppState,
    util::serde_strint::string_or_int_to_option_i32,
};
use anyhow::{anyhow, bail, Error};
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::{info, warn};

const LIST_ALL_URL: &str = "https://api.porkbun.com/api/json/v3/domain/listAll";

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
    start: usize,
    /// The default order is by expiry date, so a renewal during the sync would move a domain between pages.
    sort_name: &'static str,
}

#[derive(Serialize)]
struct FindRequest<'a> {
    apikey: &'a str,
    secretapikey: &'a str,
    domain: &'a str,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ListAllResponse {
    status: String,
    count: Option<usize>,
    domains: Option<Vec<PorkbunDomain>>,
}

impl From<PorkbunDomain> for Domain {
    fn from(domain: PorkbunDomain) -> Self {
        let metadata = json!({
            "status": domain.status,
            "tld": domain.tld,
            "create_date": domain.create_date,
            "expire_date": domain.expire_date,
            "security_lock": domain.security_lock,
        });
        let ext_expiry_at = domain.expire_date.as_deref().and_then(parse_porkbun_date);
        let ext_registered_at = domain.create_date.as_deref().and_then(parse_porkbun_date);
        let ext_auto_renew = domain.auto_renew.map(|x| x == 1);
        let ext_whois_privacy = domain.whois_privacy.map(|x| x == 1);

        Domain::new(
            domain.domain.clone(),
            "porkbun".to_string(),
            domain.domain,
            ext_expiry_at,
            ext_registered_at,
            ext_auto_renew,
            ext_whois_privacy,
            Some(metadata),
        )
    }
}

fn parse_porkbun_date(date: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    match NaiveDateTime::parse_from_str(date, "%Y-%m-%d %H:%M:%S") {
        Ok(naive) => Some(naive.and_utc()),
        Err(error) => {
            tracing::error!("Error parsing Porkbun date {date}: {error}");
            None
        }
    }
}

impl PorkbunService {
    fn keys(&self) -> Result<(&str, &str), Error> {
        let api_key = self
            .config
            .api_key
            .as_deref()
            .ok_or_else(|| anyhow!("Missing api_key"))?;
        let secret_key = self
            .config
            .secret_key
            .as_deref()
            .ok_or_else(|| anyhow!("Missing secret_key"))?;
        Ok((api_key, secret_key))
    }

    /// Fails unless the body reports `SUCCESS` with an explicit `domains` array, so a malformed answer never reads as "no domains".
    async fn post_list_all(
        &self,
        client: &reqwest::Client,
        request: &impl Serialize,
        what: &str,
    ) -> Result<Vec<PorkbunDomain>, Error> {
        let response = client.post(LIST_ALL_URL).json(request).send().await?;
        let status = response.status();
        let text = response.text().await?;
        if !status.is_success() {
            bail!("Porkbun listAll {what} failed: {text}");
        }
        let page: ListAllResponse = serde_json::from_str(&text)?;
        if page.status != "SUCCESS" {
            bail!("Porkbun listAll {what} returned status {}", page.status);
        }
        let domains = page
            .domains
            .ok_or_else(|| anyhow!("Porkbun listAll {what} has no domains array"))?;
        if let Some(count) = page.count {
            if count != domains.len() {
                bail!(
                    "Porkbun listAll {what} reported {count} domains but returned {}",
                    domains.len()
                );
            }
        }
        Ok(domains)
    }

    async fn list_page(
        &self,
        client: &reqwest::Client,
        start: usize,
    ) -> Result<Vec<PorkbunDomain>, Error> {
        let (apikey, secretapikey) = self.keys()?;
        let request = ListAllRequest {
            apikey,
            secretapikey,
            start,
            sort_name: "domain",
        };
        self.post_list_all(client, &request, &format!("at {start}"))
            .await
    }

    /// Only an empty answer confirms that `name` is gone; any other answer than that one domain is an error, so the caller keeps the stored row.
    async fn find_by_name(
        &self,
        client: &reqwest::Client,
        name: &str,
    ) -> Result<Option<PorkbunDomain>, Error> {
        let (apikey, secretapikey) = self.keys()?;
        let request = FindRequest {
            apikey,
            secretapikey,
            domain: name,
        };
        let mut found = self
            .post_list_all(client, &request, &format!("for {name}"))
            .await?;
        if found.len() > 1 || found.first().is_some_and(|domain| domain.domain != name) {
            bail!(
                "Porkbun listAll for {name} answered {} domains, starting with {:?}",
                found.len(),
                found.first().map(|domain| &domain.domain)
            );
        }
        Ok(found.pop())
    }

    /// Reads pages until Porkbun returns an empty one, and a failed page fails the whole listing; an empty page in the middle still ends it early, which the by-name check of each missed stored name covers.
    async fn list_all(&self, client: &reqwest::Client) -> Result<Vec<PorkbunDomain>, Error> {
        let mut domains = Vec::new();
        let mut names = HashSet::new();
        loop {
            let page = self.list_page(client, domains.len()).await?;
            if page.is_empty() {
                return Ok(domains);
            }
            for domain in page {
                if !names.insert(domain.domain.clone()) {
                    bail!("Porkbun listed {} twice", domain.domain);
                }
                domains.push(domain);
            }
        }
    }
}

/// Based on https://porkbun.com/api/json/v3/documentation#
impl DomainService for PorkbunService {
    async fn ingest_domains(&self, state: &AppState) -> Result<Vec<Synced>, Error> {
        let client = reqwest::Client::new();
        let listed = self.list_all(&client).await?;
        let names: HashSet<String> = listed.iter().map(|domain| domain.domain.clone()).collect();
        let mut result_domains: Vec<Synced> = listed
            .into_iter()
            .map(|domain| Synced::Listed(Domain::from(domain)))
            .collect();

        // Porkbun reports no totals, so a domain deleted between two pages shifts the offsets and leaves a live domain out of the listing; each stored name the listing misses is checked by name before the sync may delete it.
        for stored in Domain::find_by_provider(state, "porkbun").await? {
            if names.contains(&stored.name) {
                continue;
            }
            match self.find_by_name(&client, &stored.name).await {
                Ok(Some(domain)) => {
                    warn!("Porkbun listing missed {}, found it by name", stored.name);
                    result_domains.push(Synced::Listed(Domain::from(domain)));
                }
                Ok(None) => {}
                Err(error) => {
                    warn!(
                        "Porkbun could not check {}, keeping stored row: {error:#}",
                        stored.name
                    );
                    result_domains.push(Synced::Unchecked(stored.name));
                }
            }
        }

        info!("Completed porkbun: {} domains", result_domains.len());

        Ok(result_domains)
    }

    async fn ingest_domain_tld_prices_if_enabled(&self, state: &AppState) -> Result<(), Error> {
        if let Some(porkbun_pricing_config) = &self.config.pricing {
            if porkbun_pricing_config.enabled {
                ingest_domain_tld_prices(state).await
            } else {
                Ok(())
            }
        } else {
            Ok(())
        }
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

        let domains = state
            .porkbun
            .as_ref()
            .unwrap()
            .ingest_domains(&state)
            .await
            .unwrap();
    }
}

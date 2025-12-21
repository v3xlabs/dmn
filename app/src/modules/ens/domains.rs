use super::{EnsDomain, EnsService};
use crate::{models::domain::Domain, modules::DomainService, state::AppState};
use anyhow::Error;
use chrono::DateTime;
use serde::Deserialize;
use serde_json::json;

impl DomainService for EnsService {
    async fn ingest_domains(&self, state: &AppState) -> Result<Vec<Domain>, Error> {
        let mut all_domains = Vec::new();
        for address in &self.config.addresses {
            let ens_domains = fetch_ens_domains(address).await?;
            for ens_domain in ens_domains {
                let domain = Domain::new(
                    ens_domain.name.clone(),
                    "ens".to_string(),
                    ens_domain.name.clone(),
                    ens_domain.expiry_date,
                    ens_domain.registration_date,
                    None,
                    None,
                    None,
                    state,
                ).await?;
                all_domains.push(domain);
            }
        }
        Ok(all_domains)
    }

    async fn ingest_domain_tld_prices_if_enabled(&self, _state: &AppState) -> Result<(), Error> {
        Ok(())
    }
}

#[derive(Deserialize, Debug)]
struct GraphResponse {
    data: GraphData,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GraphData {
    domains: Vec<GraphDomain>,
    wrapped_domains: Vec<GraphWrappedDomain>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GraphDomain {
    name: String,
    expiry_date: Option<String>,
    created_at: Option<String>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GraphWrappedDomain {
    name: String,
    expiry_date: Option<String>,
    domain: GraphDomainInner,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct GraphDomainInner {
    created_at: Option<String>,
    expiry_date: Option<String>,
}

pub async fn fetch_ens_domains(address: &str) -> Result<Vec<EnsDomain>, Error> {
    let client = reqwest::Client::new();
    let query = json!({
        "query": r#"
            query getDomains($owner: String!) {
                domains(where: { owner: $owner }) {
                    name
                    expiryDate
                    createdAt
                }
                wrappedDomains(where: { owner: $owner }) {
                    name
                    expiryDate
                    domain {
                        createdAt
                        expiryDate
                    }
                }
            }
        "#,
        "variables": {
            "owner": address.to_lowercase()
        }
    });

    let res = client
        .post("https://api.thegraph.com/subgraphs/name/ensdomains/ens")
        .json(&query)
        .send()
        .await?
        .json::<GraphResponse>()
        .await?;

    let mut domains = Vec::new();

    for d in res.data.domains {
        let expiry_date = d.expiry_date.and_then(|e| e.parse::<i64>().ok())
            .and_then(|ts| DateTime::from_timestamp(ts, 0));
        let registration_date = d.created_at.and_then(|e| e.parse::<i64>().ok())
            .and_then(|ts| DateTime::from_timestamp(ts, 0));

        domains.push(EnsDomain {
            name: d.name,
            expiry_date,
            registration_date,
        });
    }

    for d in res.data.wrapped_domains {
        // Use domain.expiryDate if available, otherwise fallback to wrappedDomain.expiryDate
        let expiry_ts = d.domain.expiry_date.or(d.expiry_date);
        let expiry_date = expiry_ts.and_then(|e| e.parse::<i64>().ok())
            .and_then(|ts| DateTime::from_timestamp(ts, 0));
        
        let registration_date = d.domain.created_at.and_then(|e| e.parse::<i64>().ok())
            .and_then(|ts| DateTime::from_timestamp(ts, 0));

        domains.push(EnsDomain {
            name: d.name,
            expiry_date,
            registration_date,
        });
    }

    Ok(domains)
}

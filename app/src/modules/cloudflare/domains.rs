use chrono::DateTime;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{models::domain::Domain, state::AppState, Error};

use super::CloudflareService;

/// The Cloudflare API caps account listings at 50 per page.
const ACCOUNTS_PER_PAGE: u32 = 50;

#[derive(Debug, Deserialize)]
struct Account {
    id: String,
    name: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CloudflareContact {
    pub administrator_id: u64,
    pub billing_id: u64,
    pub registrant_id: u64,
    pub technical_id: u64,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CloudflareDomain {
    pub actionable_metadata: Vec<String>,
    pub administrator_contact_id: u64,
    pub auto_renew: bool,
    pub available: bool,
    pub billing_contact_id: u64,
    pub can_register: bool,
    pub cloudflare_dns: bool,
    pub cloudflare_registration: bool,
    pub contacts: CloudflareContact,
    contacts_updated_at: String, // "2025-04-16T03:49:11.736Z\",
    cor_changes: Option<String>, // unknown
    cor_locked: bool,
    cor_locked_until: Option<String>, // unknown
    cor_responses_pending: u64,
    /// "Cloudflare"
    current_registrar: String,
    domain_move: serde_json::Value, //{\"ineligibility_reasons\":[]},
    ds_records: Vec<serde_json::Value>, // unknown
    email_verified: bool,
    expires_at: String,        // \"2026-04-16T23:59:59.000Z\",
    last_known_status: String, // \"registrationActive\",
    locked: bool,
    name: String,               // \"mytestweb.site\",
    name_servers: Vec<String>,  //[\"simone.ns.cloudflare.com\",\"thomas.ns.cloudflare.com\"]
    payment_expires_at: String, // \"2026-04-16T23:59:59.000Z\",
    pending_transfer: bool,
    permissions: Vec<String>, //[\"contact_read\",\"contact_write\",\"domain_renew\",\"domain_transfer_out\",\"nameserver_write\",\"domain_delete\"]
    policies: serde_json::Value, //{\"suspension\":{\"parked\":false,\"parking_reason\":null,\"payment_expired\":false}},
    premium_type: String,        //\"not_premium\",
    privacy: bool,
    registered_at: String,              //\"2025-04-16T03:49:07.665Z\",
    registered_billing_version: String, //\"V3\",
    registrant_contact_id: u64,
    registry: String,           //\"Centralnic\",
    registry_object_id: String, //\"D544299396-CNIC\",
    registry_statuses: String,  //\"clienttransferprohibited\",
    supported_tld: bool,
    technical_contact_id: u64,
    transfer_conditions: serde_json::Value, //{\"exists\":true,\"not_premium\":true,\"not_secure\":true,\"not_started\":true,\"not_waiting\":false,\"supported_tld\":true}
}

impl CloudflareService {
    /// Cloudflare rows are only upserted, so a domain that leaves the account keeps its stored row.
    pub async fn upsert_domains(&self, state: &AppState) -> Result<(), Error> {
        let accounts = self
            .list_all("accounts", Some(ACCOUNTS_PER_PAGE), |account: &Account| {
                &account.id
            })
            .await?;
        info!("Cloudflare accounts: {}", accounts.len());

        let mut result_domains: Vec<Domain> = Vec::new();

        for account in accounts {
            let path = format!("accounts/{}/registrar/domains", account.id);
            let domains = self
                .list_all(&path, None, |domain: &CloudflareDomain| &domain.name)
                .await?;
            info!(
                "Cloudflare registrar domains in {}: {}",
                account.id,
                domains.len()
            );

            for domain in domains {
                let mut metadata = serde_json::to_value(&domain).unwrap();

                // Add account details to the metadata object
                if let Some(obj) = metadata.as_object_mut() {
                    obj.insert("account_id".to_string(), serde_json::json!(account.id));
                    obj.insert("account_name".to_string(), serde_json::json!(account.name));
                }

                let ext_expiry_at = DateTime::parse_from_rfc3339(&domain.expires_at)
                    .ok()
                    .map(|x| x.to_utc());
                let ext_registered_at = DateTime::parse_from_rfc3339(&domain.registered_at)
                    .ok()
                    .map(|x| x.to_utc());

                let ext_auto_renew = Some(domain.auto_renew);
                let ext_whois_privacy = Some(domain.privacy);

                let domain = Domain::new(
                    domain.name.clone(),
                    "cloudflare".to_string(),
                    domain.name,
                    ext_expiry_at,
                    ext_registered_at,
                    ext_auto_renew,
                    ext_whois_privacy,
                    Some(metadata),
                );

                info!("Cloudflare domain ingested: {:?}", domain);

                result_domains.push(domain);
            }
        }

        info!("Completed cloudflare");

        let mut tx = state.database.pool.begin().await?;
        for domain in &result_domains {
            domain.save(&mut tx).await?;
        }
        tx.commit().await?;

        Ok(())
    }
}

impl CloudflareDomain {
    pub async fn ingest(&self, state: &AppState) {
        // let domain = Domain::new(self.);
    }
}

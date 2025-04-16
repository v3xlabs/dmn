use cloudflare::endpoints::account::{list_accounts::ListAccountsParams, ListAccounts};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{models::domain::Domain, modules::DomainService, state::AppState, Error};

use super::{
    domains_endpoint::{ListDomains, ListDomainsParams},
    CloudflareService,
};

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
    expires_at: String, // \"2026-04-16T23:59:59.000Z\",
    last_known_status: String, // \"registrationActive\",
    locked: bool,
    name: String, // \"mytestweb.site\",
    name_servers: Vec<String>, //[\"simone.ns.cloudflare.com\",\"thomas.ns.cloudflare.com\"]
    payment_expires_at: String, // \"2026-04-16T23:59:59.000Z\",
    pending_transfer: bool,
    permissions: Vec<String>, //[\"contact_read\",\"contact_write\",\"domain_renew\",\"domain_transfer_out\",\"nameserver_write\",\"domain_delete\"]
    policies: serde_json::Value, //{\"suspension\":{\"parked\":false,\"parking_reason\":null,\"payment_expired\":false}},
    premium_type: String, //\"not_premium\",
    privacy: bool,
    registered_at: String, //\"2025-04-16T03:49:07.665Z\",
    registered_billing_version: String, //\"V3\",
    registrant_contact_id: u64,
    registry: String, //\"Centralnic\",
    registry_object_id: String, //\"D544299396-CNIC\",
    registry_statuses: String, //\"clienttransferprohibited\",
    supported_tld: bool,
    technical_contact_id: u64,
    transfer_conditions: serde_json::Value, //{\"exists\":true,\"not_premium\":true,\"not_secure\":true,\"not_started\":true,\"not_waiting\":false,\"supported_tld\":true}
}

impl DomainService for CloudflareService {
    async fn ingest_domains(&self, state: &AppState) -> Result<(), Error> {
        let accounts = ListAccounts {
            params: Some(ListAccountsParams::default()),
        };
        let accounts = self.client.request(&accounts).await?;
        info!("Cloudflare accounts: {:?}", accounts);

        for account in accounts.result {
            let domains = ListDomains {
                params: ListDomainsParams {
                    account: account.id,
                },
            };
            let domains_result = self.client.request(&domains).await?;

            info!("Cloudflare domains result: {:?}", serde_json::to_string(&domains_result).unwrap());

            let domains = domains_result.result.into_vec();

            for domain in domains {
                let metadata = serde_json::to_value(&domain).unwrap();
                let domain = Domain::new(domain.name.clone(), "cloudflare".to_string(), domain.name, Some(metadata), state).await.unwrap();

                info!("Cloudflare domain ingested: {:?}", domain);
            }
        }

        info!("Completed cloudflare");

        Ok(())
    }
}


impl CloudflareDomain {
    pub async fn ingest(&self, state: &AppState) {
        // let domain = Domain::new(self.);
    }
}

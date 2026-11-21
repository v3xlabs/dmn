use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{models::dns_domain::DnsDomain, modules::DNSService, state::AppState, Error};

use super::CloudflareService;

/// The Cloudflare API caps zone listings at 50 per page.
pub const ZONES_PER_PAGE: u32 = 50;

/// Every zone the credential can see; only `CloudflareService::get_zones` builds one, so a partial page can never reach the stored zones.
#[derive(Debug)]
pub struct ZoneListing(Vec<Zone>);

impl ZoneListing {
    pub fn into_zones(self) -> Vec<Zone> {
        self.0
    }
}

impl CloudflareService {
    pub async fn get_zones(&self) -> Result<ZoneListing, Error> {
        self.list_all("zones", Some(ZONES_PER_PAGE), |zone: &Zone| &zone.name)
            .await
            .map(ZoneListing)
    }
}

/// Reads only the fields dmn uses: the `cloudflare` crate's `Zone` fails on a pending zone, whose `activated_on` is null.
#[derive(Debug, Deserialize)]
pub struct Zone {
    pub id: String,
    pub name: String,
    #[serde(flatten)]
    pub metadata: ZoneMetadata,
}

/// Stored as `dns_domains.metadata` and read back by the web UI.
#[derive(Debug, Serialize, Deserialize)]
pub struct ZoneMetadata {
    pub status: String,
    #[serde(default)]
    pub name_servers: Vec<String>,
}

impl From<Zone> for DnsDomain {
    fn from(zone: Zone) -> Self {
        Self {
            name: zone.name,
            provider: "cloudflare".to_string(),
            external_id: Some(zone.id),
            metadata: Some(
                serde_json::to_value(zone.metadata)
                    .expect("ZoneMetadata holds only strings, so it always serializes"),
            ),
        }
    }
}

impl DNSService for CloudflareService {
    async fn ingest_dns_domains(&self, state: &AppState) -> Result<(), Error> {
        let listing = self.get_zones().await?;
        let count = DnsDomain::replace_cloudflare_zones(state, listing).await?;
        info!("Cloudflare zones ingested: {count}");
        Ok(())
    }
}

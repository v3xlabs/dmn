use std::collections::HashSet;

use anyhow::{anyhow, bail};
use cloudflare::framework::endpoint::{serialize_query, spec::EndpointSpec, Method};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::info;

use crate::{models::dns_domain::DnsDomain, modules::DNSService, state::AppState, Error};

use super::CloudflareService;

/// The Cloudflare API caps zone listings at 50 per page.
pub const ZONES_PER_PAGE: u32 = 50;

/// Lists zones with only the fields dmn reads: the `cloudflare` crate's `Zone` fails the whole page on a pending zone, whose `activated_on` is null.
/// The body is read raw because the crate's envelope type drops `success`.
#[derive(Debug, Serialize)]
pub struct ListZones {
    pub page: u32,
    pub per_page: u32,
}

impl EndpointSpec for ListZones {
    const IS_RAW_BODY: bool = true;

    type JsonResponse = ();
    type ResponseType = Vec<u8>;

    fn method(&self) -> Method {
        Method::GET
    }

    fn path(&self) -> String {
        "zones".to_string()
    }

    fn query(&self) -> Option<String> {
        serialize_query(self)
    }
}

#[derive(Debug, Deserialize)]
struct ZoneEnvelope {
    success: bool,
    #[serde(default)]
    errors: Vec<Value>,
    result: Option<Vec<Zone>>,
    result_info: Option<ResultInfo>,
}

#[derive(Debug, Deserialize)]
struct ResultInfo {
    page: u32,
    total_pages: u32,
    total_count: usize,
}

#[derive(Debug)]
pub struct ZonePage {
    zones: Vec<Zone>,
    info: ResultInfo,
}

/// Every zone the credential can see; only `CloudflareService::get_zones` builds one, so a partial page can never reach the stored zones.
#[derive(Debug)]
pub struct ZoneListing(Vec<Zone>);

impl ZoneListing {
    pub fn into_zones(self) -> Vec<Zone> {
        self.0
    }
}

impl CloudflareService {
    /// Rejects a 2xx body that reports failure, carries errors or lacks paging info, because any of them could make a partial listing look complete.
    pub async fn fetch_zone_page(&self, page: u32) -> Result<ZonePage, Error> {
        let list_zones = ListZones {
            page,
            per_page: ZONES_PER_PAGE,
        };
        let body = self.client.request(&list_zones).await?;
        let envelope: ZoneEnvelope = serde_json::from_slice(&body)?;
        if !envelope.success || !envelope.errors.is_empty() {
            bail!(
                "Cloudflare zone listing failed: success {}, errors {:?}",
                envelope.success,
                envelope.errors
            );
        }
        let info = envelope
            .result_info
            .ok_or_else(|| anyhow!("Cloudflare zone listing has no result_info"))?;
        if info.page != page {
            bail!(
                "Cloudflare returned zone page {} for page {page}",
                info.page
            );
        }
        let zones = envelope
            .result
            .ok_or_else(|| anyhow!("Cloudflare zone listing has no result"))?;
        Ok(ZonePage { zones, info })
    }

    /// Returns a listing only when every page reports the same totals and the zone count matches them; a listing that reports zero zones is complete.
    pub async fn get_zones(&self) -> Result<ZoneListing, Error> {
        let ZonePage { zones: first, info } = self.fetch_zone_page(1).await?;
        let (total_pages, total_count) = (info.total_pages, info.total_count);
        let mut zones = Vec::new();
        let mut names = HashSet::new();
        let mut batch = first;
        let mut page = 1;
        loop {
            for zone in batch {
                if !names.insert(zone.name.clone()) {
                    bail!("Cloudflare listed zone {} twice", zone.name);
                }
                zones.push(zone);
            }
            if page >= total_pages {
                break;
            }
            page += 1;
            let next = self.fetch_zone_page(page).await?;
            if (next.info.total_pages, next.info.total_count) != (total_pages, total_count) {
                bail!(
                    "Cloudflare zone totals changed during the listing: {total_count} zones in {total_pages} pages, then {} in {}",
                    next.info.total_count,
                    next.info.total_pages
                );
            }
            batch = next.zones;
        }
        if zones.len() != total_count {
            bail!(
                "Cloudflare listed {} zones but reported {total_count}",
                zones.len()
            );
        }
        Ok(ZoneListing(zones))
    }
}

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

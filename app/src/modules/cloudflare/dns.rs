use cloudflare::endpoints::zones::zone::{ListZones, ListZonesParams};
use tracing::info;

use crate::{modules::DNSService, Error, state::AppState};

use super::CloudflareService;

impl DNSService for CloudflareService {
    async fn ingest_dns_domains(&self, state: &AppState) -> Result<(), Error> {
        let list_zones = ListZones {
            params: ListZonesParams::default(),
        };

        let zones = self.client.request(&list_zones).await?;

        let peek = zones.result[0..2].as_ref();
        info!("Cloudflare zones: {:?}", peek);

        // TODO: Ingest DNS Zone into Database

        Ok(())
    }
}

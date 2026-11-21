use serde::Deserialize;
use serde_json::Value;
use strum::{Display, EnumString};

use crate::{models::dns_domain::DnsDomain, modules::cloudflare::dns::ZoneMetadata};

#[derive(Debug, Display, EnumString)]
#[strum(serialize_all = "title_case", ascii_case_insensitive)]
pub enum ZoneStatus {
    Active,
    Pending,
    Initializing,
    Moved,
    Deleted,
    Deactivated,
    #[strum(default)]
    Other(String),
}

#[derive(Debug)]
pub struct CloudflareZone {
    pub status: ZoneStatus,
    pub nameservers: Vec<String>,
}

impl TryFrom<&DnsDomain> for CloudflareZone {
    type Error = serde_json::Error;

    fn try_from(domain: &DnsDomain) -> Result<Self, Self::Error> {
        let metadata = ZoneMetadata::deserialize(domain.metadata.as_ref().unwrap_or(&Value::Null))?;
        Ok(Self {
            status: metadata
                .status
                .parse()
                .expect("ZoneStatus has a default variant, so parsing cannot fail"),
            nameservers: metadata.name_servers,
        })
    }
}

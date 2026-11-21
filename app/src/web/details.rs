use serde::Deserialize;
use serde_json::Value;
use tracing::warn;

use crate::modules::ens::EnsVersion;

use super::provider::ProviderKind;

/// Keys that `PorkbunService::ingest_domains` writes into `domains.metadata`.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct PorkbunDetails {
    status: Option<String>,
    security_lock: Option<i64>,
}

/// Keys of the serialized `CloudflareDomain` plus the account fields that `CloudflareService::ingest_domains` adds.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct CloudflareDetails {
    account_id: Option<String>,
    account_name: Option<String>,
    current_registrar: Option<String>,
    last_known_status: Option<String>,
    locked: Option<bool>,
    name_servers: Vec<String>,
    registry: Option<String>,
    registry_object_id: Option<String>,
    registry_statuses: Option<String>,
}

#[derive(Debug)]
pub enum Details {
    Porkbun(PorkbunDetails),
    Cloudflare(CloudflareDetails),
    Ens(Option<EnsVersion>),
    Unknown,
}

pub struct DetailField<'a> {
    pub label: &'static str,
    pub value: &'a str,
    pub mono: bool,
}

impl Details {
    pub fn new(provider: &ProviderKind, metadata: Option<&Value>, domain: &str) -> Self {
        let Some(metadata) = metadata else {
            return Self::Unknown;
        };
        let parsed = match provider {
            ProviderKind::Porkbun => PorkbunDetails::deserialize(metadata).map(Self::Porkbun),
            ProviderKind::Cloudflare => {
                CloudflareDetails::deserialize(metadata).map(Self::Cloudflare)
            }
            ProviderKind::Ens => return Self::Ens(EnsVersion::from_metadata(metadata)),
            ProviderKind::Other(_) => return Self::Unknown,
        };
        parsed.unwrap_or_else(|error| {
            warn!(domain, provider = %provider, %error, "domain metadata does not match provider shape");
            Self::Unknown
        })
    }

    pub fn transfer_lock(&self) -> Option<bool> {
        match self {
            Self::Porkbun(details) => details.security_lock.map(|lock| lock != 0),
            Self::Cloudflare(details) => details.locked,
            Self::Ens(_) | Self::Unknown => None,
        }
    }

    pub fn registrar(&self) -> Option<&str> {
        match self {
            Self::Cloudflare(details) => details.current_registrar.as_deref(),
            Self::Porkbun(_) | Self::Ens(_) | Self::Unknown => None,
        }
    }

    pub fn nameservers(&self) -> &[String] {
        match self {
            Self::Cloudflare(details) => &details.name_servers,
            Self::Porkbun(_) | Self::Ens(_) | Self::Unknown => &[],
        }
    }

    pub fn manage_url(&self, domain: &str) -> Option<String> {
        match self {
            Self::Porkbun(_) => Some("https://porkbun.com/account/domainsSpeedy".to_string()),
            Self::Cloudflare(details) => details.account_id.as_ref().map(|account| {
                format!("https://dash.cloudflare.com/{account}/registrar/domain/{domain}")
            }),
            Self::Ens(_) => Some(format!("https://app.ens.domains/{domain}")),
            Self::Unknown => None,
        }
    }

    pub fn fields(&self) -> Vec<DetailField<'_>> {
        let candidates: Vec<(&'static str, Option<&str>, bool)> = match self {
            Self::Porkbun(details) => vec![("Status", details.status.as_deref(), false)],
            Self::Cloudflare(details) => vec![
                ("Status", details.last_known_status.as_deref(), false),
                ("Registrar", details.current_registrar.as_deref(), false),
                ("Registry", details.registry.as_deref(), false),
                ("Registry ID", details.registry_object_id.as_deref(), true),
                (
                    "Registry statuses",
                    details.registry_statuses.as_deref(),
                    true,
                ),
                ("Account", details.account_name.as_deref(), false),
            ],
            Self::Ens(version) => vec![("Version", version.map(EnsVersion::badge), true)],
            Self::Unknown => Vec::new(),
        };
        candidates
            .into_iter()
            .filter_map(|(label, value, mono)| {
                let value = value.map(|v| v.trim()).filter(|v| !v.is_empty())?;
                Some(DetailField { label, value, mono })
            })
            .collect()
    }
}

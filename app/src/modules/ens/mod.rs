use std::{fmt, str::FromStr, time::Duration};

use chrono::{DateTime, Utc};
use figment::{providers::Env, Figment, Provider};
use serde::{
    de::{self, Deserializer, SeqAccess, Visitor},
    Deserialize, Serialize,
};
use tracing::{info, warn};

pub mod abi;
#[cfg(feature = "ens-discovery")]
pub mod discovery;
pub mod domains;
pub mod resolve;
pub mod rpc;
pub mod schedule;

use abi::keccak256;
#[cfg(feature = "ens-discovery")]
use discovery::DEFAULT_ENS_DISCOVERY_URL;
use rpc::DEFAULT_ENS_RPC_URL;

/// Caps Ethereum RPC and optional discovery HTTP calls at 30 seconds so hung public nodes cannot stall CLI commands or the refresh loop indefinitely.
const ENS_HTTP_TIMEOUT_SECS: u64 = 30;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct EnsName(String);

impl EnsName {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn label(&self) -> &str {
        self.0
            .strip_suffix(".eth")
            .expect("EnsName holds a second-level .eth name")
    }

    pub fn namehash(&self) -> [u8; 32] {
        let mut node = [0_u8; 32];
        let mut buf = [0_u8; 64];
        for label in self.0.rsplit('.') {
            let lh = keccak256(label.as_bytes());
            buf[..32].copy_from_slice(&node);
            buf[32..].copy_from_slice(&lh);
            node = keccak256(&buf);
        }
        node
    }
}

impl FromStr for EnsName {
    type Err = &'static str;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let normalized = raw.trim().trim_end_matches('.').to_ascii_lowercase();
        match normalized.strip_suffix(".eth") {
            Some(label) if !label.is_empty() && !label.contains('.') => Ok(Self(normalized)),
            _ => Err("only second-level .eth names are tracked"),
        }
    }
}

impl fmt::Display for EnsName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvmAddress(String);

impl EvmAddress {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn to_bytes(&self) -> [u8; 20] {
        let bytes = hex::decode(&self.0[2..]).expect("EvmAddress holds 40 validated hex chars");
        bytes.try_into().expect("EvmAddress decodes to 20 bytes")
    }
}

impl FromStr for EvmAddress {
    type Err = &'static str;

    fn from_str(raw: &str) -> Result<Self, Self::Err> {
        let trimmed = raw.trim();
        let hex_part = trimmed
            .strip_prefix("0x")
            .or_else(|| trimmed.strip_prefix("0X"))
            .ok_or("missing 0x prefix")?;
        if hex_part.len() != 40 || !hex_part.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("invalid Ethereum address");
        }
        Ok(Self(format!("0x{}", hex_part.to_ascii_lowercase())))
    }
}

impl fmt::Display for EvmAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Deserialize)]
struct RawEnsConfig {
    #[serde(default, deserialize_with = "deserialize_string_list")]
    names: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_string_list")]
    addresses: Vec<String>,
    rpc_url: Option<String>,
    #[cfg(feature = "ens-discovery")]
    discovery_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnsConfig {
    pub names: Vec<EnsName>,
    pub addresses: Vec<EvmAddress>,
    pub rpc_url: Option<String>,
    #[cfg(feature = "ens-discovery")]
    pub discovery_url: Option<String>,
}

impl From<RawEnsConfig> for EnsConfig {
    fn from(raw: RawEnsConfig) -> Self {
        let mut names = Vec::new();
        for entry in raw.names {
            match entry.parse::<EnsName>() {
                Ok(name) => {
                    if !names.contains(&name) {
                        names.push(name);
                    }
                }
                Err(reason) => warn!(ens.name = %entry, reason, "skipping ENS name in config"),
            }
        }

        let mut addresses = Vec::new();
        for entry in raw.addresses {
            match entry.parse::<EvmAddress>() {
                Ok(addr) => {
                    if !addresses.contains(&addr) {
                        addresses.push(addr);
                    }
                }
                Err(_) => warn!(address = %entry, "ignoring invalid Ethereum address in config"),
            }
        }

        let rpc_url = raw
            .rpc_url
            .map(|url| url.trim().to_string())
            .filter(|url| !url.is_empty());

        #[cfg(feature = "ens-discovery")]
        let discovery_url = raw
            .discovery_url
            .map(|url| url.trim().trim_end_matches('/').to_string())
            .filter(|url| !url.is_empty());

        Self {
            names,
            addresses,
            rpc_url,
            #[cfg(feature = "ens-discovery")]
            discovery_url,
        }
    }
}

impl EnsConfig {
    fn extract(provider: &impl Provider) -> Result<Self, Box<figment::Error>> {
        Figment::new()
            .merge(Figment::from(provider).focus("ens"))
            .merge(Env::prefixed("ENS_"))
            .extract::<RawEnsConfig>()
            .map(Self::from)
            .map_err(Box::new)
    }
}

fn deserialize_string_list<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    struct ListVisitor;

    impl<'de> Visitor<'de> for ListVisitor {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a comma-separated string or a sequence of strings")
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            Ok(v.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(String::from)
                .collect())
        }

        fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
            self.visit_str(&v)
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
            let mut values = Vec::new();
            while let Some(value) = seq.next_element::<String>()? {
                let trimmed = value.trim();
                if !trimmed.is_empty() {
                    values.push(trimmed.to_string());
                }
            }
            Ok(values)
        }
    }

    deserializer.deserialize_any(ListVisitor)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnsVersion {
    V1,
    Upgraded,
    V2,
}

impl EnsVersion {
    pub fn from_metadata(meta: &serde_json::Value) -> Option<Self> {
        meta.get("ens_version")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::V1 => "v1",
            Self::Upgraded => "upgraded",
            Self::V2 => "v2",
        }
    }

    pub fn badge(self) -> (&'static str, &'static str) {
        match self {
            Self::V1 => ("v1", "bg-amber-100 text-amber-800"),
            Self::Upgraded => ("v1->v2", "bg-sky-100 text-sky-800"),
            Self::V2 => ("v2", "bg-emerald-100 text-emerald-800"),
        }
    }
}

impl fmt::Display for EnsVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct EnsMetadata {
    pub status: String,
    pub ens_version: EnsVersion,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authority: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub namehash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manager: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grace_ends_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub v1_expiry_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wrapper_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolver: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
pub struct EnsDomain {
    pub name: String,
    pub expiry_date: DateTime<Utc>,
    pub registration_date: Option<DateTime<Utc>>,
    pub metadata: EnsMetadata,
}

impl fmt::Display for EnsDomain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Name:         {}", self.name)?;
        writeln!(f, "Version:      {}", self.metadata.ens_version)?;
        if let Some(authority) = &self.metadata.authority {
            writeln!(f, "Authority:    {authority}")?;
        }
        writeln!(f, "Status:       {}", self.metadata.status)?;
        if let Some(owner) = &self.metadata.owner {
            writeln!(f, "Owner:        {owner}")?;
        }
        if let Some(manager) = &self.metadata.manager {
            writeln!(f, "Manager:      {manager}")?;
        }
        writeln!(f, "Expires:      {}", self.expiry_date.to_rfc3339())?;
        if let Some(grace) = self.metadata.grace_ends_at {
            writeln!(f, "Grace ends:   {}", grace.to_rfc3339())?;
        }
        if let Some(v1_expiry) = self.metadata.v1_expiry_at {
            writeln!(f, "V1 expiry:    {}", v1_expiry.to_rfc3339())?;
        }
        if let Some(wrapper_state) = &self.metadata.wrapper_state {
            writeln!(f, "Wrapper:      {wrapper_state}")?;
        }
        if let Some(resolver) = &self.metadata.resolver {
            writeln!(f, "Resolver:     {resolver}")?;
        }
        Ok(())
    }
}

pub struct EnsService {
    pub config: EnsConfig,
    pub client: reqwest::Client,
}

impl EnsService {
    pub fn new(config: EnsConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(ENS_HTTP_TIMEOUT_SECS))
            .user_agent(concat!("dmn/", env!("CARGO_PKG_VERSION")))
            .build()
            .expect("reqwest client with rustls-tls and static user agent must build");
        Self { config, client }
    }

    pub fn rpc_url(&self) -> &str {
        self.config
            .rpc_url
            .as_deref()
            .unwrap_or(DEFAULT_ENS_RPC_URL)
    }

    #[cfg(feature = "ens-discovery")]
    pub fn discovery_url(&self) -> &str {
        self.config
            .discovery_url
            .as_deref()
            .unwrap_or(DEFAULT_ENS_DISCOVERY_URL)
    }

    pub fn for_lookup(provider: &impl Provider) -> Result<Self, Box<figment::Error>> {
        EnsConfig::extract(provider).map(Self::new)
    }

    pub async fn try_init(provider: &impl Provider) -> Option<Self> {
        match EnsConfig::extract(provider) {
            Ok(config) => {
                let service = Self::new(config);
                if service.config.names.is_empty() && service.config.addresses.is_empty() {
                    warn!("ENS config found but neither names nor valid addresses are configured");
                    return None;
                }
                info!(
                    ens.name_count = service.config.names.len(),
                    ens.address_count = service.config.addresses.len(),
                    ens.rpc_url = %service.rpc_url(),
                    "ENS config verified"
                );
                Some(service)
            }
            Err(err) => {
                warn!(error = ?err, "ENS config extraction failed");
                None
            }
        }
    }
}

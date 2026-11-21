use std::{collections::HashMap, fmt};

use anyhow::{Context, Error};
use serde::Deserialize;

/// IANA RDAP bootstrap registry for domain names, RFC 9224.
pub const IANA_RDAP_BOOTSTRAP_URL: &str = "https://data.iana.org/rdap/dns.json";

/// Host of `IANA_RDAP_BOOTSTRAP_URL`, for pacing.
pub const IANA_RDAP_BOOTSTRAP_HOST: &str = "data.iana.org";

/// RDAP bases for TLDs that run RDAP but are missing from the IANA bootstrap, each checked by hand on 2026-11-21.
/// An entry stops mattering once IANA lists the TLD, because the bootstrap wins.
const RDAP_OVERRIDES: &[(&str, &str)] = &[
    ("ch", "https://rdap.nic.ch/"),
    ("co", "https://rdap.registry.co/co/"),
    ("de", "https://rdap.denic.de/"),
    ("io", "https://rdap.identitydigital.services/rdap/"),
];

#[derive(Deserialize)]
struct RawBootstrap {
    services: Vec<(Vec<String>, Vec<String>)>,
}

/// RDAP base URL per TLD, from the IANA bootstrap plus `RDAP_OVERRIDES`.
#[derive(Debug, Default)]
pub struct Bootstrap {
    bases: HashMap<String, String>,
}

/// No IANA bootstrap has been fetched yet. No single name can fix this, so the indexer ends its step instead of failing names.
#[derive(Debug)]
pub struct BootstrapUnavailable;

impl fmt::Display for BootstrapUnavailable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("IANA RDAP bootstrap unavailable")
    }
}

impl std::error::Error for BootstrapUnavailable {}

impl Bootstrap {
    pub fn parse(json: &str) -> Result<Self, serde_json::Error> {
        let raw: RawBootstrap = serde_json::from_str(json)?;
        let mut bases = HashMap::new();
        for (tlds, urls) in raw.services {
            let Some(url) = urls
                .iter()
                .find(|url| url.starts_with("https://"))
                .or_else(|| urls.first())
            else {
                continue;
            };
            let base = if url.ends_with('/') {
                url.clone()
            } else {
                format!("{url}/")
            };
            for tld in tlds {
                bases.insert(tld.to_ascii_lowercase(), base.clone());
            }
        }
        Ok(Self { bases })
    }

    pub async fn fetch(client: &reqwest::Client) -> Result<Self, Error> {
        let body = client
            .get(IANA_RDAP_BOOTSTRAP_URL)
            .send()
            .await?
            .error_for_status()?
            .text()
            .await?;
        Self::parse(&body).context("IANA RDAP bootstrap is not valid JSON")
    }

    pub fn base_url(&self, tld: &str) -> Option<&str> {
        self.bases.get(tld).map(String::as_str).or_else(|| {
            RDAP_OVERRIDES
                .iter()
                .find(|(listed, _)| *listed == tld)
                .map(|(_, base)| *base)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_wins_over_override_and_bases_end_in_slash() {
        let bootstrap = Bootstrap::parse(
            r#"{"services": [
                [["com", "net"], ["https://rdap.verisign.com/com/v1/"]],
                [["io"], ["http://plain.example", "https://rdap.example/io"]]
            ]}"#,
        )
        .unwrap();

        assert_eq!(
            bootstrap.base_url("net"),
            Some("https://rdap.verisign.com/com/v1/")
        );
        assert_eq!(bootstrap.base_url("io"), Some("https://rdap.example/io/"));
        assert_eq!(bootstrap.base_url("de"), Some("https://rdap.denic.de/"));
        assert_eq!(bootstrap.base_url("me"), None);
    }
}

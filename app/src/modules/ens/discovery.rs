use anyhow::{anyhow, Context, Error};
use chrono::{Duration, Utc};
use reqwest::{StatusCode, Url};
use serde::Deserialize;

use super::{EnsName, EvmAddress};

pub const DEFAULT_ENS_DISCOVERY_URL: &str = "https://api.bigname.sh";

/// Matches the bigname `/v1/addresses/{address}/names` documented upper limit (`[1, 200]`) to minimize HTTP round-trips.
const DISCOVERY_PAGE_SIZE: usize = 200;
/// Bounds pagination at 20,000 names per pass so a stuck upstream cursor cannot loop forever; exceeding this returns an error to avoid partial deletions.
const DISCOVERY_MAX_PAGES: usize = 100;
/// Retries up to 3 times on bigname HTTP `409 stale` responses emitted when a new block publishes mid-pagination.
const DISCOVERY_MAX_STALE_RETRIES: usize = 3;
/// Queries 35 days back on `relation=former_owner` to cover the 28-day ENSv2 grace period plus a 7-day buffer without scanning ancient releases.
const ENSV2_GRACE_LOOKBACK_DAYS: i64 = 35;

#[derive(Deserialize)]
struct DiscoveryResponse {
    data: Vec<DiscoveryRow>,
    page: DiscoveryPage,
}

#[derive(Deserialize)]
struct DiscoveryPage {
    next_cursor: Option<String>,
    has_more: bool,
}

#[derive(Deserialize)]
struct DiscoveryLapsed {
    release_kind: Option<String>,
}

#[derive(Deserialize)]
struct DiscoveryRow {
    name: String,
    status: Option<String>,
    lapsed_registration: Option<DiscoveryLapsed>,
}

impl DiscoveryRow {
    fn into_candidate_name(self, is_former_owner: bool) -> Option<EnsName> {
        let name = self.name.parse::<EnsName>().ok()?;
        if name.as_str().contains('[') {
            return None;
        }
        let status = self
            .status
            .as_deref()
            .unwrap_or("active")
            .to_ascii_lowercase();
        if is_former_owner {
            let unregistered = self
                .lapsed_registration
                .as_ref()
                .and_then(|l| l.release_kind.as_deref())
                .is_some_and(|k| k.eq_ignore_ascii_case("unregistered"));
            (status == "expired" && !unregistered).then_some(name)
        } else {
            (status == "active" || status == "expired").then_some(name)
        }
    }
}

pub struct DiscoveryClient<'a> {
    client: &'a reqwest::Client,
    base_url: &'a str,
}

impl<'a> DiscoveryClient<'a> {
    pub fn new(client: &'a reqwest::Client, base_url: &'a str) -> Self {
        Self { client, base_url }
    }

    pub async fn discover_names(&self, address: &EvmAddress) -> Result<Vec<EnsName>, Error> {
        let mut names = self
            .fetch_pages(address, "owner,manager", None, false)
            .await?;
        let grace_cutoff = (Utc::now() - Duration::days(ENSV2_GRACE_LOOKBACK_DAYS)).timestamp();
        names.extend(
            self.fetch_pages(address, "former_owner", Some(grace_cutoff), true)
                .await?,
        );
        Ok(names)
    }

    async fn fetch_pages(
        &self,
        address: &EvmAddress,
        relation: &str,
        expires_after: Option<i64>,
        is_former_owner: bool,
    ) -> Result<Vec<EnsName>, Error> {
        let mut names = Vec::new();
        let mut cursor: Option<String> = None;

        for page_index in 0..DISCOVERY_MAX_PAGES {
            let mut url = Url::parse(self.base_url)
                .with_context(|| format!("invalid ENS discovery URL: {}", self.base_url))?;
            {
                let mut segments = url.path_segments_mut().map_err(|()| {
                    anyhow!("ENS discovery URL cannot be a base: {}", self.base_url)
                })?;
                segments.extend(["v1", "addresses", address.as_str(), "names"]);
            }
            {
                let mut query = url.query_pairs_mut();
                query.append_pair("namespace", "ens");
                query.append_pair("relation", relation);
                query.append_pair("dedupe", "name");
                query.append_pair("page_size", &DISCOVERY_PAGE_SIZE.to_string());
                if let Some(after) = expires_after {
                    query.append_pair("expires_after", &after.to_string());
                }
                if let Some(token) = &cursor {
                    query.append_pair("cursor", token);
                }
            }

            let (status, body) = self.get_with_retry(url).await?;
            if !status.is_success() {
                return Err(anyhow!(
                    "ENS address discovery returned {status} for {address} ({relation}): {body}"
                ));
            }

            let response: DiscoveryResponse = serde_json::from_str(&body)
                .context("failed to parse ENS address discovery response")?;
            names.extend(
                response
                    .data
                    .into_iter()
                    .filter_map(|row| row.into_candidate_name(is_former_owner)),
            );

            let next_cursor = response
                .page
                .has_more
                .then_some(response.page.next_cursor)
                .flatten();
            match next_cursor {
                Some(next) => {
                    if cursor.as_deref() == Some(next.as_str()) {
                        return Err(anyhow!(
                            "ENS discovery returned repeating cursor for {address} ({relation})"
                        ));
                    }
                    if page_index + 1 == DISCOVERY_MAX_PAGES {
                        return Err(anyhow!(
                            "ENS discovery exceeded {DISCOVERY_MAX_PAGES} pages for {address} ({relation})"
                        ));
                    }
                    cursor = Some(next);
                }
                None => return Ok(names),
            }
        }

        unreachable!("loop returns on None cursor or DISCOVERY_MAX_PAGES overflow")
    }

    async fn get_with_retry(&self, url: Url) -> Result<(StatusCode, String), Error> {
        for attempt in 0..=DISCOVERY_MAX_STALE_RETRIES {
            let response = self
                .client
                .get(url.clone())
                .send()
                .await
                .with_context(|| format!("ENS address discovery request failed for {url}"))?;
            let status = response.status();
            let body = response
                .text()
                .await
                .with_context(|| format!("failed to read ENS discovery response for {url}"))?;
            if status == StatusCode::CONFLICT && attempt < DISCOVERY_MAX_STALE_RETRIES {
                continue;
            }
            return Ok((status, body));
        }

        unreachable!("loop returns on final attempt")
    }
}

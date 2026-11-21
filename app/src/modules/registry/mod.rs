use std::{
    collections::HashMap,
    fmt,
    time::{Duration, Instant},
};

use anyhow::{anyhow, Error};
use chrono::{DateTime, Utc};
use serde::Serialize;
use strum::Display;

pub mod bootstrap;
pub mod name;
pub mod rdap;
pub mod record;
pub mod whois;

use bootstrap::{Bootstrap, BootstrapUnavailable};
use name::DomainName;
use record::RegistryRecord;

/// IANA serves `dns.json` with `cache-control: max-age=86400`, so the bootstrap and WHOIS referrals are fetched again after a day.
const REFERRAL_TTL_SECS: u64 = 24 * 60 * 60;

/// Wait before asking IANA again after a failed bootstrap or referral fetch, so an IANA outage costs one request per hour, not one per lookup.
const REFERRAL_RETRY_SECS: u64 = 60 * 60;

/// Redirects followed for one name before the lookup fails. RDAP redirects from a registry to a registrar take one hop, so a longer chain is a loop or a misconfiguration.
pub const MAX_RDAP_REDIRECTS: u8 = 3;

/// Caps one RDAP request so a hung registry cannot stall a lookup.
const RDAP_TIMEOUT_SECS: u64 = 20;

/// The protocol of a `Route`, as stored next to a snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Display, sqlx::Type)]
#[strum(serialize_all = "lowercase")]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
pub enum Source {
    Rdap,
    Whois,
}

/// Where the registry data for one TLD comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "source", content = "server", rename_all = "lowercase")]
pub enum Route {
    /// RDAP base URL, ending in `/`.
    Rdap(String),
    /// A full RDAP domain URL that a server redirected to.
    #[serde(rename = "rdap")]
    RdapUrl(String),
    /// Port-43 WHOIS host.
    Whois(String),
}

impl Route {
    pub fn source(&self) -> Source {
        match self {
            Self::Rdap(_) | Self::RdapUrl(_) => Source::Rdap,
            Self::Whois(_) => Source::Whois,
        }
    }

    /// RDAP base URL or WHOIS host.
    pub fn server(&self) -> &str {
        match self {
            Self::Rdap(server) | Self::RdapUrl(server) | Self::Whois(server) => server,
        }
    }

    /// The server that rate limits apply to, as one key per server: lower case, no trailing dot, and the port only when it is not the scheme's default.
    pub fn host(&self) -> String {
        match self {
            Self::Rdap(url) | Self::RdapUrl(url) => match reqwest::Url::parse(url) {
                Ok(url) => {
                    let host = url.host_str().unwrap_or_default().trim_end_matches('.');
                    match url.port() {
                        Some(port) => format!("{host}:{port}"),
                        None => host.to_string(),
                    }
                }
                Err(_) => url.to_ascii_lowercase(),
            },
            Self::Whois(host) => host.trim_end_matches('.').to_ascii_lowercase(),
        }
    }
}

/// One registry answer: the normalized record plus the raw RDAP JSON or WHOIS text.
#[derive(Debug, Clone, Serialize)]
pub struct Lookup {
    pub name: DomainName,
    #[serde(flatten)]
    pub route: Route,
    pub record: RegistryRecord,
    #[serde(skip)]
    pub raw: String,
}

/// A cached IANA answer and when to ask again.
struct Cached<T> {
    value: T,
    refresh_at: Instant,
}

impl<T> Cached<T> {
    fn fresh(&self) -> bool {
        Instant::now() < self.refresh_at
    }
}

/// The one IANA request a route needs before it is known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IanaQuery {
    Bootstrap,
    /// The WHOIS referral for this TLD.
    Referral(String),
}

impl IanaQuery {
    /// The host that `Registry::query_iana` sends this request to.
    pub fn host(&self) -> &'static str {
        match self {
            Self::Bootstrap => bootstrap::IANA_RDAP_BOOTSTRAP_HOST,
            Self::Referral(_) => whois::IANA_WHOIS_HOST,
        }
    }
}

/// What `Registry::route` knows for a name without sending a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteStep {
    /// `None` when neither the bootstrap, the overrides nor IANA name a server for the TLD.
    Known(Option<Route>),
    Ask(IanaQuery),
}

/// RDAP first through the IANA bootstrap and `RDAP_OVERRIDES`, then port-43 WHOIS through the IANA referral.
pub struct Registry {
    client: reqwest::Client,
    bootstrap: Cached<Option<Bootstrap>>,
    referrals: HashMap<String, Cached<Result<Option<String>, String>>>,
}

impl Registry {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(RDAP_TIMEOUT_SECS))
            .user_agent(concat!("dmn/", env!("CARGO_PKG_VERSION")))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("reqwest client with rustls-tls and static user agent must build");
        Self {
            client,
            bootstrap: Cached {
                value: None,
                refresh_at: Instant::now(),
            },
            referrals: HashMap::new(),
        }
    }

    /// Resolves and fetches in one call, following up to `MAX_RDAP_REDIRECTS` redirects, for a one-off lookup that needs no pacing.
    pub async fn lookup(&mut self, name: &DomainName) -> Result<Lookup, Error> {
        loop {
            match self.route(name)? {
                RouteStep::Known(Some(route)) => return self.fetch_following(route, name).await,
                RouteStep::Known(None) => {
                    return Err(anyhow!("no RDAP or WHOIS server known for .{}", name.tld()))
                }
                RouteStep::Ask(query) => self.query_iana(&query).await?,
            }
        }
    }

    /// Answers from cached IANA data only and never sends a request.
    ///
    /// # Errors
    /// `BootstrapUnavailable` while no bootstrap is cached and the next fetch is not due yet, and the cached error of a failed WHOIS referral.
    pub fn route(&self, name: &DomainName) -> Result<RouteStep, Error> {
        if !self.bootstrap.fresh() {
            return Ok(RouteStep::Ask(IanaQuery::Bootstrap));
        }
        let bootstrap = self.bootstrap.value.as_ref().ok_or(BootstrapUnavailable)?;
        let tld = name.tld();
        if let Some(base) = bootstrap.base_url(tld) {
            return Ok(RouteStep::Known(Some(Route::Rdap(base.to_string()))));
        }
        match self.referrals.get(tld).filter(|cached| cached.fresh()) {
            Some(cached) => match &cached.value {
                Ok(host) => Ok(RouteStep::Known(host.clone().map(Route::Whois))),
                Err(reason) => Err(Error::msg(reason.clone())),
            },
            None => Ok(RouteStep::Ask(IanaQuery::Referral(tld.to_string()))),
        }
    }

    /// Sends one request to `query.host()` and caches the answer. A failure is cached for `REFERRAL_RETRY_SECS`.
    pub async fn query_iana(&mut self, query: &IanaQuery) -> Result<(), Error> {
        match query {
            IanaQuery::Bootstrap => match Bootstrap::fetch(&self.client).await {
                Ok(fresh) => {
                    self.bootstrap = Cached {
                        value: Some(fresh),
                        refresh_at: Instant::now() + Duration::from_secs(REFERRAL_TTL_SECS),
                    };
                    Ok(())
                }
                Err(err) => {
                    self.bootstrap.refresh_at =
                        Instant::now() + Duration::from_secs(REFERRAL_RETRY_SECS);
                    Err(err.context("IANA RDAP bootstrap fetch failed"))
                }
            },
            IanaQuery::Referral(tld) => {
                let (value, ttl) = match whois::query(whois::IANA_WHOIS_HOST, tld).await {
                    Ok(answer) => (Ok(whois::referral(&answer)), REFERRAL_TTL_SECS),
                    Err(err) => (
                        Err(format!("IANA WHOIS referral for .{tld} failed: {err:#}")),
                        REFERRAL_RETRY_SECS,
                    ),
                };
                let result = value.clone().map(|_| ()).map_err(Error::msg);
                self.referrals.insert(
                    tld.clone(),
                    Cached {
                        value,
                        refresh_at: Instant::now() + Duration::from_secs(ttl),
                    },
                );
                result
            }
        }
    }

    async fn fetch_following(&self, mut route: Route, name: &DomainName) -> Result<Lookup, Error> {
        for _ in 0..=MAX_RDAP_REDIRECTS {
            match self.fetch(&route, name).await {
                Err(err) => match err.downcast::<rdap::Redirected>() {
                    Ok(redirect) => route = Route::RdapUrl(redirect.to),
                    Err(err) => return Err(err),
                },
                found => return found,
            }
        }
        Err(anyhow!(
            "more than {MAX_RDAP_REDIRECTS} RDAP redirects for {name}"
        ))
    }

    /// Sends one request to `route.host()`. An RDAP redirect comes back as `rdap::Redirected`.
    pub async fn fetch(&self, route: &Route, name: &DomainName) -> Result<Lookup, Error> {
        let url = match route {
            Route::Rdap(base) => format!("{base}domain/{name}"),
            Route::RdapUrl(url) => url.clone(),
            Route::Whois(host) => return Lookup::whois(host, name).await,
        };
        let (record, raw) = rdap::query(&self.client, &url, &route.host(), name).await?;
        Ok(Lookup {
            name: name.clone(),
            route: route.clone(),
            record,
            raw,
        })
    }
}

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for Lookup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let record = &self.record;
        let now = Utc::now();
        let date = |at: Option<DateTime<Utc>>| match at {
            Some(at) => format!(
                "{} ({})",
                at.to_rfc3339(),
                chrono_humanize::HumanTime::from(at - now)
            ),
            None => "unknown".to_string(),
        };
        writeln!(f, "Name:         {}", self.name)?;
        writeln!(
            f,
            "Source:       {} {}",
            self.route.source(),
            self.route.server()
        )?;
        if !record.registered {
            return writeln!(f, "Registered:   no");
        }
        writeln!(f, "Expires:      {}", date(record.expiry_at))?;
        writeln!(f, "Created:      {}", date(record.registered_at))?;
        if let Some(registrar) = &record.registrar {
            writeln!(f, "Registrar:    {registrar}")?;
        }
        if let Some(dnssec) = record.dnssec {
            writeln!(
                f,
                "DNSSEC:       {}",
                if dnssec { "signed" } else { "unsigned" }
            )?;
        }
        for (index, status) in record.statuses.iter().flatten().enumerate() {
            let label = if index == 0 { "Status:" } else { "" };
            writeln!(f, "{label:<14}{status}")?;
        }
        for (index, nameserver) in record.nameservers.iter().flatten().enumerate() {
            let label = if index == 0 { "Nameservers:" } else { "" };
            writeln!(f, "{label:<14}{nameserver}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_keys_match_across_spelling_and_redirects() {
        let bootstrap = Route::Rdap("http://LOCALHOST:19073/rdap/".to_string());
        let redirect =
            Route::RdapUrl("http://localhost:19073/rdap/domain/example.test".to_string());
        assert_eq!(bootstrap.host(), "localhost:19073");
        assert_eq!(redirect.host(), bootstrap.host());

        assert_eq!(
            Route::Rdap("https://RDAP.Example.COM./".to_string()).host(),
            "rdap.example.com"
        );
        assert_eq!(
            Route::Rdap("https://rdap.example.com:443/".to_string()).host(),
            "rdap.example.com"
        );
        assert_eq!(
            Route::Whois("WHOIS.NIC.AT.".to_string()).host(),
            "whois.nic.at"
        );
    }
}

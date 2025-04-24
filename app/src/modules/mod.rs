use crate::{models::domain::Domain, state::AppState, Error};

pub mod cloudflare;
pub mod porkbun;
pub mod whois;
pub mod domains;
pub mod ntfy;

pub trait DomainService {
    async fn ingest_domains(&self, state: &AppState) -> Result<Vec<Domain>, Error>;

    async fn ingest_domain_tld_prices(&self, state: &AppState) -> Result<(), Error>;
}

pub trait DNSService {
    async fn ingest_dns_domains(&self, state: &AppState) -> Result<(), Error>;
}

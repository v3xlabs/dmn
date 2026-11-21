use crate::{models::domain::Domain, state::AppState, Error};

pub mod cloudflare;
pub mod domains;
pub mod ens;
pub mod ntfy;
pub mod porkbun;
pub mod registry;

pub enum Synced {
    Listed(Domain),
    /// The provider could not check this name, so its stored row stays exactly as it is.
    Unchecked(String),
    /// The domain leaves tracking; this last known state is diffed against the stored row before the row is deleted.
    Departed(Domain),
}

pub trait DomainService {
    async fn ingest_domains(&self, state: &AppState) -> Result<Vec<Synced>, Error>;

    async fn ingest_domain_tld_prices_if_enabled(&self, state: &AppState) -> Result<(), Error>;
}

pub trait DNSService {
    async fn ingest_dns_domains(&self, state: &AppState) -> Result<(), Error>;
}

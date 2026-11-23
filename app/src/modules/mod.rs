use crate::{models::domain::Domain, state::AppState, Error};

pub mod cloudflare;
pub mod domains;
pub mod ens;
pub mod index;
pub mod manual;
pub mod ntfy;
pub mod porkbun;
pub mod registry;

/// What a provider found for one name it checked.
/// A stored name with no outcome stays exactly as it is.
pub enum Synced {
    Listed(Domain),
    /// The provider checked this name and no longer has it, so its stored row is deleted.
    Gone(String),
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

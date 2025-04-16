use crate::{Error, state::AppState};

pub mod cloudflare;
pub mod porkbun;

pub trait DomainService {
    async fn ingest_domains(&self, state: &AppState) -> Result<(), Error>;
}

pub trait DNSService {
    async fn ingest_dns_domains(&self, state: &AppState) -> Result<(), Error>;
}

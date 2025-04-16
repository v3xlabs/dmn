use async_std::prelude::FutureExt;
use modules::{DNSService, DomainService};
use state::{AppState, AppStateInner};
use std::sync::Arc;

pub mod cache;
pub mod database;
pub mod models;
pub mod modules;
pub mod server;
pub mod state;
pub mod util;

pub type Error = anyhow::Error;

#[async_std::main]
async fn main() {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt::init();
    tracing::info!("Starting dmn");

    let state: AppState = Arc::new(AppStateInner::init().await);

    if let Some(porkbun) = &state.porkbun {
        porkbun.ingest_domains(&state).await.unwrap();
    }

    if let Some(cloudflare) = &state.cloudflare {
        cloudflare.ingest_dns_domains(&state).await.unwrap();
        cloudflare.ingest_domains(&state).await.unwrap();
    }

    let http = server::start_http(state.clone());

    let cache_size_notifier = state.cache.collect(&state);

    cache_size_notifier.race(http).await;
}

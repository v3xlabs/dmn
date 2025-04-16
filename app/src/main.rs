use async_std::prelude::FutureExt;
use state::{AppState, AppStateInner};
use std::sync::Arc;

pub mod cache;
pub mod database;
pub mod models;
pub mod server;
pub mod state;
// pub mod util;

#[async_std::main]
async fn main() {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt::init();

    let state: AppState = Arc::new(AppStateInner::init().await);

    let http = server::start_http(state.clone());

    let cache_size_notifier = state.cache.collect(&state);

    cache_size_notifier.race(http).await;
}

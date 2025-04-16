use std::{num::NonZero, sync::Arc};

use governor::Quota;
// use channel::ChannelApi;
// use info::InfoApi;
// use media::MediaApi;
use opentelemetry::global;
// use party::PartyApi;
use poem::{
    endpoint::StaticFilesEndpoint, get, handler, listener::TcpListener,
    middleware::OpenTelemetryMetrics, EndpointExt, Route, Server,
};
use poem_openapi::{payload::Html, OpenApi, OpenApiService, Tags};

// use maps::MapsApi;
use ratelimit::GovRateLimitMiddleware;
use tracing::info;

use crate::state::AppState;
// use auth::{oauth::OAuthApi, AuthApi};
// use bm::BattleMetricsApi;
// use inventory::InventoryApi;

// pub mod auth;
// pub mod bm;
// pub mod inventory;
// pub mod maps;
// pub mod party;
pub mod ratelimit;

#[derive(Tags)]
pub enum ApiTags {
    /// Party Related Operations
    Party,
    /// Maps Related Operations
    /// 
    /// This uses the RustMaps.com API to search for maps
    Maps,
    /// Inventory Related Operations
    /// 
    /// This uses the scmm.app API to get inventory information
    Inventory,
    /// BattleMetrics Related Operations
    /// 
    /// This uses the BattleMetrics.com API to get server information
    BattleMetrics,
    /// Auth Authentication
    Auth,
}

fn get_api(state: AppState) -> impl OpenApi {
    (
        // PartyApi,
        // MapsApi,
        // AuthApi,
        // OAuthApi::new(state.clone()),
        // BattleMetricsApi,
        // InventoryApi,
    )
}

pub async fn start_http(state: AppState) {
    info!("Starting HTTP server");
    let api_service = OpenApiService::new(get_api(state.clone()), "dmn", "0.0.1")
        .server("http://localhost:3000/api");

    let spec = api_service.spec_endpoint();

    let limiter = GovRateLimitMiddleware::new(
        Quota::per_minute(NonZero::new(120).unwrap()),
        Quota::per_minute(NonZero::new(60).unwrap()),
    );

    let api_service = api_service
        .with(limiter)
        // .with(TraceId::new(Arc::new(global::tracer("dmn"))))
        .with(OpenTelemetryMetrics::new());


    let path = std::path::Path::new("./www");

    let spa_endpoint = StaticFilesEndpoint::new(path)
        .show_files_listing()
        .index_file("index.html")
        .fallback_to_index();

    let app = Route::new()
        .nest("/", spa_endpoint)
        .nest("/openapi.json", spec)
        // .at("/prom", get(prom::route))
        .nest("/docs", get(get_openapi_docs))
        .nest("/api", api_service)
        // .at("/v/:video_id", get(redirect::video_redirect))
        // .at("/v/:video_id/oembed", get(redirect::video_oembed))
        // .nest("/metrics", get(prom::route))
        // .with(OpenTelemetryTracing::new(global::tracer("storedvideo")))
        .data(state);
    // .with(Cors::new());

    Server::new(TcpListener::bind("0.0.0.0:3000"))
        .run(app)
        .await
        .unwrap();
}

#[handler]
async fn get_openapi_docs() -> Html<&'static str> {
    Html(include_str!("./index.html"))
}

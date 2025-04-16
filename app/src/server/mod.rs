use std::num::NonZero;

use domains::DomainApi;
use governor::Quota;
use poem::{
    endpoint::StaticFilesEndpoint, get, handler, listener::TcpListener,
    middleware::OpenTelemetryMetrics, EndpointExt, Route, Server,
};
use poem_openapi::{payload::Html, OpenApi, OpenApiService, Tags};

use ratelimit::GovRateLimitMiddleware;
use tracing::info;

use crate::state::AppState;

pub mod domains;
pub mod ratelimit;

#[derive(Tags)]
pub enum ApiTags {
    /// Domain Related Operations
    Domains,
    /// DNS Related Operations
    DNS,
}

fn get_api(state: AppState) -> impl OpenApi {
    DomainApi
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

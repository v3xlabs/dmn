use std::{num::NonZero, sync::Arc};

use domains::DomainApi;
use governor::Quota;
use poem::{
    endpoint::StaticFilesEndpoint, get, handler, listener::TcpListener,
    middleware::OpenTelemetryMetrics, web::Data, EndpointExt, Response, Route, Server,
};
use poem_openapi::{payload::Html, OpenApi, OpenApiService, Tags};

use ratelimit::GovRateLimitMiddleware;
use serde_json::Value;
use tracing::info;

use crate::{state::AppState, web};

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

#[derive(Debug, Clone)]
pub struct OpenApiSpec {
    pub spec: String,
}

/// Reorders the tags in the OpenAPI spec according to the specified order without
/// parsing the entire JSON. Only the tags array is modified.
/// Tags not in the order list will be placed at the end.
fn reorder_openapi_tags(json: &str, tag_order: &[&str]) -> String {
    // Find the position of the tags array
    let tags_start = match json.find(r#""tags": ["#) {
        Some(pos) => pos,
        None => return json.to_string(), // No tags found
    };

    // The key and opening bracket
    let key_length = r#""tags": "#.len();
    let content_start = tags_start + key_length;

    // Now we need to find where the array ends
    // We'll count brackets to handle nested structures
    let mut bracket_count = 1; // We start after the opening '['
    let mut content_end = content_start + 1;

    for (idx, ch) in json[content_start + 1..].char_indices() {
        if ch == '[' {
            bracket_count += 1;
        } else if ch == ']' {
            bracket_count -= 1;
            if bracket_count == 0 {
                content_end = content_start + 1 + idx + 1; // +1 to include the closing bracket
                break;
            }
        }
    }

    if bracket_count != 0 {
        return json.to_string(); // Malformed JSON
    }

    // Extract the array content including brackets
    let tags_array = &json[content_start..content_end];

    // Parse just the tags array
    let tags_result: Result<Vec<Value>, _> = serde_json::from_str(tags_array);

    match tags_result {
        Ok(mut tags) => {
            // Reorder the tags array
            let mut ordered_tags = Vec::new();
            let mut remaining_tags = Vec::new();

            // First, collect tags in the specified order
            for &tag_name in tag_order {
                let position = tags.iter().position(|tag| {
                    if let Some(name) = tag.get("name") {
                        name.as_str() == Some(tag_name)
                    } else {
                        false
                    }
                });

                if let Some(idx) = position {
                    ordered_tags.push(tags.remove(idx));
                }
            }

            // Append any remaining tags
            remaining_tags.append(&mut tags);
            ordered_tags.append(&mut remaining_tags);

            // Serialize just the tags array back to a string
            if let Ok(new_tags_json) = serde_json::to_string(&ordered_tags) {
                // Reconstruct the JSON string with the reordered tags
                format!(
                    "{}{}{}",
                    &json[..content_start],
                    new_tags_json,
                    &json[content_end..]
                )
            } else {
                json.to_string()
            }
        }
        Err(_) => json.to_string(),
    }
}

pub async fn start_http(state: AppState) {
    info!("Starting HTTP server");

    let description = include_str!("./README.md");
    let cargo_version = env!("CARGO_PKG_VERSION");
    let api_service = OpenApiService::new(get_api(state.clone()), "dmn", cargo_version)
        .description(description)
        .server("http://localhost:3000/api");
    // write spec_json to file in www/openapi.json
    let spec_json = api_service.spec();

    // Define the desired tag order
    let tag_order = &["Domains", "DNS", "Whois"];

    // Reorder tags according to the specified order
    let spec_json = reorder_openapi_tags(&spec_json, tag_order);

    // Prepare spec_json for use in the route
    let spec_json_str = spec_json.clone();

    let openapi_spec = OpenApiSpec {
        spec: spec_json_str,
    };

    let spec_route = Route::new()
        .at("", get(get_openapi_spec))
        .data(Arc::new(openapi_spec));

    let limiter = GovRateLimitMiddleware::new(
        Quota::per_minute(NonZero::new(120).unwrap()),
        Quota::per_minute(NonZero::new(60).unwrap()),
    );

    let api_service = api_service
        .with(limiter)
        // .with(TraceId::new(Arc::new(global::tracer("dmn"))))
        .with(OpenTelemetryMetrics::new());

    let path = std::path::Path::new("./www");

    // let spa_endpoint = StaticFilesEndpoint::new(path)
    //     .show_files_listing()
    //     .index_file("index.html")
    //     .fallback_to_index();

    let app = Route::new()
        .nest("/", web::web_endpoint)
        .nest("/openapi.json", spec_route)
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

#[handler]
async fn get_openapi_spec(payload: Data<&Arc<OpenApiSpec>>) -> Response {
    let spec = payload.spec.clone();
    Response::builder()
        .header("Content-Type", "application/json")
        .body(spec)
}

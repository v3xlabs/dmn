mod details;
mod drawer;
mod expiry;
mod history;
mod icons;
mod page;
mod provider;
mod row;
mod zone;

use chrono::Utc;
use poem::{error::InternalServerError, web::Data};
use poem_openapi::payload::Html;

use crate::models::{dns_domain::DnsDomain, domain::Domain, notification::Notification};
use crate::state::AppState;

use self::page::PageData;

/// Each page response inlines this many history entries per domain, so page size stays bounded as notifications accumulate.
const HISTORY_PER_DOMAIN: u32 = 20;

#[poem::handler]
pub async fn web_endpoint(state: Data<&AppState>) -> poem::Result<Html<String>> {
    let domains = Domain::get_all(&state).await.map_err(InternalServerError)?;
    let notifications = Notification::latest_per_domain(&state, HISTORY_PER_DOMAIN)
        .await
        .map_err(InternalServerError)?;
    // Stored zones outlive a removed token, so they only count while Cloudflare is configured.
    let zones = match state.cloudflare {
        Some(_) => DnsDomain::find_by_provider(&state, "cloudflare")
            .await
            .map_err(InternalServerError)?,
        None => Vec::new(),
    };
    let page = PageData::new(domains, notifications, zones, Utc::now());

    Ok(Html(page.render().into_string()))
}

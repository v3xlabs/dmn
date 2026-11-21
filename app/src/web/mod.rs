mod details;
mod drawer;
mod expiry;
mod history;
mod icons;
mod page;
mod provider;
mod row;

use chrono::Utc;
use poem::{error::InternalServerError, web::Data};
use poem_openapi::payload::Html;

use crate::models::{domain::Domain, notification::Notification};
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
    let page = PageData::new(domains, notifications, Utc::now());

    Ok(Html(page.render().into_string()))
}

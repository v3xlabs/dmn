use std::collections::HashMap;

use crate::{models::domain::Domain, server::ApiTags, state::AppState};
use chrono::{DateTime, Duration, DurationRound, Utc};
use icalendar::{Calendar, Component, Event, EventLike};
use poem::web::Data;
use poem_openapi::{
    payload::{PlainText},
    ApiResponse, OpenApi, ResponseContent,
};

#[derive(ResponseContent)]
enum IcsContent {
    #[oai(content_type = "text/calendar")]
    Calendar(PlainText<String>),
}

#[derive(ApiResponse)]
enum CalendarResponse {
    #[oai(status = 200)]
    Ok(IcsContent),
}

pub struct CalApi;

#[OpenApi]
impl CalApi {
    #[oai(path = "/domains.ics", method = "get", tag = "ApiTags::Calendar")]
    async fn get_cal(&self, state: Data<&AppState>) -> CalendarResponse {
        let calendar = generate_calendar(&state).await;

        CalendarResponse::Ok(IcsContent::Calendar(PlainText(calendar.to_string())))
    }
}

pub struct CalendarConfig {
    pub enabled: Option<bool>,      // default true
    pub round_to_day: Option<bool>, // default true
}

async fn generate_calendar(state: &AppState) -> Calendar {
    let round_to_day = true;

    let mut calendar = Calendar::new();

    let domains = Domain::get_all(state).await.unwrap();

    let mut batched_events = HashMap::<String, Vec<String>>::new();

    for domain in domains {
        let expiry_date = domain.ext_expiry_at.unwrap();
        let expiry_date = if round_to_day {
            expiry_date.duration_trunc(Duration::days(1)).unwrap()
        } else {
            expiry_date.duration_trunc(Duration::hours(1)).unwrap()
        };

        let events = batched_events.entry(expiry_date.to_string()).or_default();
        events.push(domain.name);
    }

    for (date, domains) in batched_events {
        let date = date.parse::<DateTime<Utc>>().unwrap();

        let summary = format!("{} domains expire on {}", domains.len(), date);
        let description = domains.join("\n");

        let mut event = Event::new();
        let mut event = event
            .summary(&summary)
            .description(&description)
            .starts(date);

        if round_to_day {
            event = event.all_day(date.date_naive());
        } else {
            event = event.starts(date).ends(date + Duration::hours(1));
        }

        calendar.push(event.done());
    }

    calendar
}

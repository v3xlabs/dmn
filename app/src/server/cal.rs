use std::collections::HashMap;

use chrono::{DateTime, Duration, DurationRound, Utc};
use icalendar::{Calendar, Component, Event, EventLike};
use poem::web::Data;
use poem_openapi::{payload::{PlainText, Response}, ApiResponse, OpenApi};

use crate::{models::domain::Domain, state::AppState};

pub struct CalApi;

#[OpenApi]
impl CalApi {
    #[oai(path = "/domains.ics", method = "get")]
    async fn get_cal(&self, state: Data<&AppState>) -> Response<PlainText<String>> {
        let calendar = generate_calendar(&state).await;

        poem_openapi::payload::Response::new(PlainText(calendar.to_string()))
            .header("Content-Type", "text/calendar; charset=utf-8")
            .header(
                "Content-Disposition",
                "attachment; filename=\"domains.ics\"",
            )
            .header("Cache-Control", "no-cache, no-store, must-revalidate")
            .header("Pragma", "no-cache")
            .header("Expires", "0")
            .header("Access-Control-Allow-Origin", "*")
    }
}

pub struct CalendarConfig {
    pub round_to_day: bool,
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

use crate::models::domain::Domain;
use crate::state::AppState;
use chrono::{DateTime, Duration, Utc};
use chrono_humanize::HumanTime;
use maud::{html, Markup, DOCTYPE};
use poem::web::Data;

pub fn provider_to_color(provider: &String) -> &str {
    match provider.as_str() {
        "porkbun" => "text-[#EF7878]",
        "cloudflare" => "text-[#F48120]",
        _ => "text-gray-500",
    }
}

pub fn expiry_to_color(expiry: &Option<DateTime<Utc>>) -> &str {
    if let Some(expiry) = expiry {
        let now = Utc::now();
        if expiry < &now {
            "text-red-500"
        } else if expiry < &now.checked_add_signed(Duration::days(30)).unwrap() {
            "text-orange-500"
        } else if expiry < &now.checked_add_signed(Duration::days(60)).unwrap() {
            "text-yellow-500"
        } else {
            "text-gray-500"
        }
    } else {
        "text-gray-500"
    }
}

#[poem::handler]
pub async fn web_endpoint(state: Data<&AppState>) -> poem_openapi::payload::Html<String> {
    let domains = match Domain::get_all(&state).await {
        Ok(domains) => domains,
        Err(_) => vec![],
    };

    let icon = maud::PreEscaped(include_str!("./public/refresh-ccw.svg"));

    let markup: Markup = html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                title { "Domains" }
                style { (include_str!("./dist/index.css")) }
            }
            body class="w-full min-h-screen bg-gray-100 p-4 overflow-y-auto font-sans" {
                div class="w-full max-w-screen-md mx-auto md:mt-16 bg-white rounded-md border border-gray-200 p-8" {
                    div class="flex gap-4" {
                        h1 class="text-2xl font-bold" { "Hello, World!" }
                    }
                    p { "Your domains:" }
                    table class="table-auto w-full mt-4" {
                        thead {
                            tr class="text-left" {
                                th { "Provider" }
                                th { "Name" }
                                th { "Expiry" }
                                th { "Registered" }
                                th { "Auto Renew" }
                            }
                        }
                        tbody {
                            @for domain in &domains {
                                tr {
                                    td class={ (provider_to_color(&domain.provider)) " text-xs" } { (domain.provider) }
                                    td { (domain.name) }
                                    td {
                                        div class={(expiry_to_color(&domain.ext_expiry_at)) " text-xs"} {
                                            (domain.ext_expiry_at.map(|dt| HumanTime::from(dt).to_string()).unwrap_or("-".to_string()))
                                        }
                                        div class="text-xs text-gray-500" {
                                            (domain.ext_expiry_at.map(|dt| dt.format("%d/%m/%y - %-I %P").to_string()).unwrap_or("-".to_string()))
                                        }
                                    }
                                    td {
                                        div class={"text-xs"} {
                                            (domain.ext_expiry_at.map(|dt| HumanTime::from(dt).to_string()).unwrap_or("-".to_string()))
                                        }
                                        div class="text-xs text-gray-500" {
                                            (domain.ext_expiry_at.map(|dt| dt.format("%d/%m/%y - %-I %P").to_string()).unwrap_or("-".to_string()))
                                        }
                                    }
                                    td {
                                        div class={"" (if domain.ext_auto_renew.unwrap_or(false) { "text-green-500" } else { "text-red-500" })} {
                                            (icon)
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    };

    poem_openapi::payload::Html(markup.into_string())
}

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

const icon: maud::PreEscaped<&str> = maud::PreEscaped(include_str!("./public/refresh-ccw.svg"));


#[poem::handler]
pub async fn web_endpoint(state: Data<&AppState>) -> poem_openapi::payload::Html<String> {
    let domains = match Domain::get_all(&state).await {
        Ok(domains) => domains,
        Err(_) => vec![],
    };

    let (already_expired_domains, active_domains): (Vec<Domain>, Vec<Domain>) = domains
        .into_iter()
        .partition(|domain| domain.ext_expiry_at.unwrap_or(Utc::now()) < Utc::now());

    let markup: Markup = html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1.0";
                title { "My Domains | dmn" }
                style { (include_str!("./dist/index.css")) }
            }
            body class="w-full min-h-screen bg-gray-100 p-4 overflow-y-auto font-sans" {
                div class="w-full max-w-screen-md mx-auto flex flex-col gap-4" {
                    div class="flex gap-4 justify-between items-center px-4 w-full" {
                        h1 class="text-2xl font-bold" { "dmn" }
                        div class="flex gap-2 items-center" {
                            a href="/api/rss.xml" class="text-blue-500 hover:underline" { "Rss" }
                            a href="/api/domains.ics" class="text-blue-500 hover:underline" { "Calendar" }
                        }
                    }
                    div class="card" {
                        (domain_table(already_expired_domains))
                    }
                    div class="card" {
                        (domain_table(active_domains))
                    }
                }
            }
        }
    };

    poem_openapi::payload::Html(markup.into_string())
}

fn domain_table(domains: Vec<Domain>) -> Markup {
    html! {
        table class="table-auto w-full" {
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
                                (domain.ext_registered_at.map(|dt| HumanTime::from(dt).to_string()).unwrap_or("-".to_string()))
                            }
                            div class="text-xs text-gray-500" {
                                (domain.ext_registered_at.map(|dt| dt.format("%d/%m/%y - %-I %P").to_string()).unwrap_or("-".to_string()))
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

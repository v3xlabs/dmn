use std::ops::Deref;

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
            "text-neutral-500"
        }
    } else {
        "text-gray-500"
    }
}

const icon_renew: maud::PreEscaped<&str> =
    maud::PreEscaped(include_str!("./public/refresh-ccw.svg"));
const icon_privacy: maud::PreEscaped<&str> = maud::PreEscaped(include_str!("./public/eye-off.svg"));
const icon_privacy_exposed: maud::PreEscaped<&str> =
    maud::PreEscaped(include_str!("./public/eye.svg"));
const porkbun_icon: maud::PreEscaped<&str> =
    maud::PreEscaped(include_str!("./public/porkbun_icon.svg"));
const cloudflare_icon: maud::PreEscaped<&str> =
    maud::PreEscaped(include_str!("./public/cloudflare_icon.svg"));

fn format_cloudflare_url(account_id: &str, domain: &str) -> String {
    format!(
        "https://dash.cloudflare.com/{}/registrar/domain/{}",
        account_id, domain
    )
}

#[poem::handler]
pub async fn web_endpoint(state: Data<&AppState>) -> poem_openapi::payload::Html<String> {
    let domains = match Domain::get_all(&state).await {
        Ok(domains) => domains,
        Err(_) => vec![],
    };

    let total_domains = domains.len();

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
                div class="w-full max-w-content mx-auto flex flex-col gap-4" {
                    div class="flex gap-4 justify-between items-baseline px-4 w-full" {
                        h1 class="text-2xl font-bold" { "Domains" }
                        div class="flex gap-4 items-baseline" {
                            a href="/api/rss.xml" class="text-blue-500 hover:underline" target="_blank" { "Rss" }
                            a href="/api/domains.ics" class="text-blue-500 hover:underline" { "Calendar" }
                            a href="/docs" class="text-blue-500 hover:underline" target="_blank" { "Docs" }
                        }
                    }
                    div class="grid grid-cols-4 gap-4" {
                        div class="card no-padding py-2 px-4 flex flex-col justify-between" {
                            h2 class="text-lg font-bold" { "Total" }
                            p class="text-2xl font-bold" { (total_domains) }
                        }
                        div class="card no-padding py-2 px-4 flex flex-col justify-between" {
                            h2 class="text-lg font-bold" { "Expired" }
                            p class="text-2xl font-bold" { (already_expired_domains.len()) }
                        }
                        div class="card no-padding py-2 px-4 flex flex-col justify-between" {
                            h2 class="text-lg font-bold" { "Expiring" }
                            span class="text-sm text-gray-500" { "in next 30 days" }
                            p class="text-2xl font-bold" { (active_domains.iter().filter(|d| d.ext_expiry_at.unwrap_or(Utc::now()) < Utc::now().checked_add_signed(Duration::days(30)).unwrap()).count()) }
                        }
                        div class="card no-padding py-2 px-4 flex flex-col justify-between" {
                            h2 class="text-lg font-bold" { "Expiring" }
                            span class="text-sm text-gray-500" { "in next year" }
                            p class="text-2xl font-bold" { (active_domains.iter().filter(|d| d.ext_expiry_at.unwrap_or(Utc::now()) < Utc::now().checked_add_signed(Duration::days(365)).unwrap()).count()) }
                        }
                    }
                    div class="card" {
                        (domain_table(already_expired_domains))
                    }
                    div class="card" {
                        (domain_table(active_domains))
                    }
                }
                footer class="w-full max-w-content mx-auto flex justify-between gap-2 items-center mb-8 px-4 py-2 text-sm" {
                    p class="flex gap-2 items-center" {
                        a href="https://github.com/v3xlabs" class="text-blue-500 hover:underline" target="_blank" { "v3xlabs" }
                        {"/"}
                        a href="https://github.com/v3xlabs/dmn" class="text-blue-500 hover:underline" target="_blank" { "dmn" }
                    }
                    span class="text-gray-500" { "v" (env!("CARGO_PKG_VERSION")) }
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
                    th class="whitespace-nowrap pr-2" { "Provider" }
                    th class="w-full min-w-0" { "Name" }
                    th class="whitespace-nowrap" { "Expiry" }
                    th class="whitespace-nowrap" { "Registered" }
                    th class="whitespace-nowrap" { "" }
                }
            }
            tbody {
                @for domain in &domains {
                    tr {
                        td class={(provider_to_color(&domain.provider)) " whitespace-nowrap pr-2"} {
                            div class="flex gap-2 items-center h-full" {
                                (match domain.provider.as_str() {
                                    "porkbun" => porkbun_icon,
                                    "cloudflare" => cloudflare_icon,
                                    _ => icon_privacy,
                                })
                                (match domain.provider.as_str() {
                                        "cloudflare" => {
                                            html!(a href={(format_cloudflare_url(&domain.metadata.as_ref().unwrap().get("account_id").map_or("-".to_string(), |id| id.as_str().unwrap().to_string()), &domain.name))} target="_blank" {
                                                span class="text-xs" { (domain.provider) }
                                            })
                                        },
                                    _ => {
                                        html!(span class="text-xs" { (domain.provider) })
                                    }
                                }
                                )
                            }
                        }
                        td class="w-full min-w-0" {
                            span class="private" { (domain.name.split(".").next().unwrap()) }
                            {"."}
                            span class="text-gray-500" { (domain.name.split(".").skip(1).collect::<Vec<&str>>().join(".")) }
                        }
                        td class="pr-4" {
                            div class={(expiry_to_color(&domain.ext_expiry_at)) " text-xs whitespace-nowrap"} {
                                (domain.ext_expiry_at.map(|dt| HumanTime::from(dt).to_string()).unwrap_or("-".to_string()))
                            }
                            div class="text-xs text-gray-500 whitespace-nowrap" {
                                (domain.ext_expiry_at.map(|dt| dt.format("%d/%m/%y - %-I %P").to_string()).unwrap_or("-".to_string()))
                            }
                        }
                        td class="pr-4" {
                            div class={"text-xs text-gray-700 whitespace-nowrap"} {
                                (domain.ext_registered_at.map(|dt| HumanTime::from(dt).to_string()).unwrap_or("-".to_string()))
                            }
                            div class="text-xs text-gray-500 whitespace-nowrap" {
                                (domain.ext_registered_at.map(|dt| dt.format("%d/%m/%y - %-I %P").to_string()).unwrap_or("-".to_string()))
                            }
                        }
                        td {
                            div class="flex gap-2 h-full items-center whitespace-nowrap" {
                                div class={"" (if domain.ext_auto_renew.unwrap_or(false) { "text-green-500" } else { "text-red-500" })} {
                                    (icon_renew)
                                }
                                div class={"" (if domain.ext_whois_privacy.unwrap_or(false) { "text-green-500" } else { "text-red-500" })} {
                                    (if domain.ext_whois_privacy.unwrap_or(false) { icon_privacy } else { icon_privacy_exposed })
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

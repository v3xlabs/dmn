use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};
use maud::{html, Markup, PreEscaped, DOCTYPE};
use tracing::warn;

use crate::models::{dns_domain::DnsDomain, domain::Domain, notification::Notification};
use crate::modules::registry::name::DomainName;

use super::expiry::{relative_past, ExpiryTone};
use super::history::Change;
use super::icons;
use super::provider::ProviderKind;
use super::registry::RegistryData;
use super::row::DomainRow;
use super::zone::CloudflareZone;

const THEME_INIT_SCRIPT: &str = r#"(function(){var s=null;try{s=localStorage.getItem("dmn-theme")}catch(e){}var d=window.matchMedia&&window.matchMedia("(prefers-color-scheme: dark)").matches;document.documentElement.classList.toggle("dark",s==="dark"||(s!=="light"&&d));})();"#;

const FAVICON_DATA_URI: &str = "data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 24 24' fill='none'%3E%3Crect width='24' height='24' rx='7' fill='%230f172a'/%3E%3Cpath d='M7 12H17M12 7V17' stroke='%23fff' stroke-width='1.8' stroke-linecap='round'/%3E%3Ccircle cx='12' cy='12' r='2.4' fill='%230f172a' stroke='%23fff' stroke-width='1.8'/%3E%3C/svg%3E";

pub struct PageData {
    rows: Vec<DomainRow>,
    zones: HashMap<String, CloudflareZone>,
    providers: BTreeMap<ProviderKind, usize>,
    now: DateTime<Utc>,
}

impl PageData {
    /// Rows keep this order on the page; the client sorts the Expires column by row index, so this is the only expiry sort rule.
    pub fn new(
        domains: Vec<Domain>,
        notifications: Vec<Notification>,
        zones: Vec<DnsDomain>,
        registry: RegistryData,
        now: DateTime<Utc>,
    ) -> Self {
        // A registrar row and a manual row of one name share one history, so a registrar handover shows on the manual row.
        let history_key = |name: &str| {
            name.parse::<DomainName>()
                .map_or_else(|_| name.to_string(), |name| name.to_string())
        };
        let mut changes: HashMap<String, Vec<Change>> = HashMap::new();
        for (name, registry_changes) in registry.changes {
            changes
                .entry(history_key(&name))
                .or_default()
                .extend(registry_changes);
        }
        for notification in &notifications {
            changes
                .entry(history_key(&notification.domain))
                .or_default()
                .push(Change::from(notification));
        }

        let zones: HashMap<String, CloudflareZone> = zones
            .iter()
            .filter_map(|zone| match CloudflareZone::try_from(zone) {
                Ok(parsed) => Some((zone.name.to_ascii_lowercase(), parsed)),
                Err(error) => {
                    warn!(
                        "Skipping Cloudflare zone {} with unreadable metadata: {error}",
                        zone.name
                    );
                    None
                }
            })
            .collect();

        let mut rows: Vec<DomainRow> = domains
            .into_iter()
            .map(|domain| {
                let history = changes
                    .remove(&history_key(&domain.name))
                    .unwrap_or_default();
                let panel = registry.panels.get(&history_key(&domain.name)).cloned();
                DomainRow::new(domain, history, panel, now)
            })
            .collect();
        rows.sort_by(|a, b| {
            let key = |row: &DomainRow| {
                (
                    matches!(
                        row.expiry.tone,
                        ExpiryTone::Never | ExpiryTone::Unpublished | ExpiryTone::Pending
                    ),
                    row.domain.ext_expiry_at,
                )
            };
            key(a)
                .cmp(&key(b))
                .then_with(|| a.domain.name.cmp(&b.domain.name))
        });

        let mut providers = BTreeMap::new();
        for row in &rows {
            *providers.entry(row.provider.clone()).or_insert(0) += 1;
        }

        Self {
            rows,
            zones,
            providers,
            now,
        }
    }

    fn zone_for(&self, row: &DomainRow) -> Option<&CloudflareZone> {
        // Cloudflare Registrar requires a Cloudflare zone and the row already shows Cloudflare as provider.
        if row.provider == ProviderKind::Cloudflare {
            return None;
        }
        self.zones.get(&row.domain.name.to_ascii_lowercase())
    }

    pub fn render(&self) -> Markup {
        html! {
            (DOCTYPE)
            html lang="en" {
                head {
                    meta charset="utf-8";
                    meta name="viewport" content="width=device-width, initial-scale=1.0";
                    meta name="color-scheme" content="light dark";
                    title { "Domains | dmn" }
                    link rel="icon" type="image/svg+xml" href=(FAVICON_DATA_URI);
                    script { (PreEscaped(THEME_INIT_SCRIPT)) }
                    style { (PreEscaped(include_str!("./dist/index.css"))) }
                }
                body {
                    a href="#main-content" class="skip-link" { "Skip to content" }
                    (self.render_header())
                    main id="main-content" class="main" {
                        (self.render_page_head())
                        @if self.rows.is_empty() {
                            div class="panel empty" {
                                p class="empty-title" { "No domains" }
                            }
                        } @else {
                            (self.render_totals())
                            (self.render_toolbar())
                            (self.render_table())
                        }
                    }
                    (self.render_footer())
                    (self.render_drawer())
                    div class="toast" id="toast" role="status" aria-live="polite" {}
                    script { (PreEscaped(include_str!("./public/app.js"))) }
                }
            }
        }
    }

    fn render_header(&self) -> Markup {
        html! {
            header class="topbar" {
                a href="/" class="brand" {
                    span class="brand-mark" { (icons::BRAND_MARK) }
                    "dmn"
                }
                div class="topbar-actions" {
                    div class="anchor" {
                        button type="button" class="glyph-btn has-tip" id="calendar-btn" aria-label="Calendar" aria-expanded="false" aria-controls="calendar-popover" {
                            (icons::CALENDAR)
                            span class="tip" { "Calendar" }
                        }
                        div class="popover" id="calendar-popover" role="dialog" aria-label="Calendar" data-open="false" {
                            p class="popover-title" { "Expiry calendar" }
                            p class="popover-url mono" id="ics-url" { "/api/domains.ics" }
                            div class="popover-actions" {
                                button type="button" class="btn btn-secondary" id="copy-ics-btn" { "Copy URL" }
                                a href="/api/domains.ics" download="domains.ics" class="btn btn-primary" { "Download" }
                            }
                        }
                    }
                    a href="/api/rss.xml" target="_blank" rel="noreferrer" class="glyph-btn has-tip hide-mobile" aria-label="RSS feed" {
                        (icons::RSS)
                        span class="tip" { "RSS feed" }
                    }
                    a href="/docs" target="_blank" rel="noreferrer" class="glyph-btn has-tip hide-mobile" aria-label="OpenAPI docs" {
                        (icons::DOCS)
                        span class="tip" { "OpenAPI docs" }
                    }
                    button type="button" class="glyph-btn glyph-raised has-tip" id="theme-toggle" aria-label="Dark mode" {
                        span class="theme-light" { (icons::MOON) }
                        span class="theme-dark" { (icons::SUN) }
                        span class="tip" {
                            span class="theme-light" { "Dark mode" }
                            span class="theme-dark" { "Light mode" }
                        }
                    }
                }
            }
        }
    }

    fn render_page_head(&self) -> Markup {
        let synced = self.rows.iter().filter_map(DomainRow::synced_at).max();
        html! {
            div class="page-head" {
                div class="page-head-title" {
                    h1 class="page-title" { "Domains" }
                    @if let Some(synced) = synced {
                        span class="sync-meta tabular" {
                            span class="pulse-dot" aria-hidden="true" {}
                            "Synced " (relative_past(synced, self.now))
                        }
                    }
                }
                a href="/" class="btn btn-primary" {
                    (icons::REFRESH)
                    "Refresh"
                }
            }
        }
    }

    fn render_totals(&self) -> Markup {
        let attention = self
            .rows
            .iter()
            .filter(|row| row.expiry.needs_attention)
            .count();
        let window = self
            .rows
            .iter()
            .filter(|row| row.expiry.within_window)
            .count();
        html! {
            div class="totals" role="group" aria-label="Filter by status" {
                (totals_cell("all", "All", None, self.rows.len()))
                (totals_cell("attention", "Attention", Some("red"), attention))
                (totals_cell("window", "Within 90d", Some("amber"), window))
            }
        }
    }

    fn render_toolbar(&self) -> Markup {
        html! {
            div class="toolbar" {
                label class="search" {
                    (icons::SEARCH)
                    input type="search" id="domain-search" placeholder="Search domains" aria-label="Search domains" autocomplete="off" spellcheck="false";
                    kbd class="search-kbd" aria-hidden="true" { "/" }
                }
                div class="anchor provider-filter" {
                    button type="button" class="provider-trigger" id="provider-trigger" aria-haspopup="listbox" aria-expanded="false" aria-controls="provider-menu" {
                        span class="provider-icon" id="provider-trigger-icon" aria-hidden="true" { (icons::ALL_PROVIDERS) }
                        span class="provider-trigger-label" id="provider-trigger-label" { "All providers" }
                        span class="provider-count tabular" id="provider-trigger-count" { (self.rows.len()) }
                        (icons::CHEVRON_DOWN)
                    }
                    div class="provider-menu" id="provider-menu" role="listbox" aria-label="Filter by provider" data-open="false" {
                        (provider_option("", icons::ALL_PROVIDERS, "All providers", self.rows.len(), true))
                        @for (provider, count) in &self.providers {
                            (provider_option(&provider.to_string(), provider.icon(), &provider.to_string(), *count, false))
                        }
                    }
                }
            }
        }
    }

    fn render_table(&self) -> Markup {
        html! {
            div class="panel" {
                table class="domain-table" aria-label="Tracked domains" {
                    thead {
                        tr {
                            (sort_header("expires", "Expires", "ascending"))
                            (sort_header("name", "Domain", "none"))
                            (sort_header("provider", "Provider", "none"))
                            th scope="col" class="col-controls" { span class="th-label" { "Controls" } }
                            (sort_header("changed", "Changes", "none"))
                        }
                    }
                    tbody id="domain-rows" {
                        @for (index, row) in self.rows.iter().enumerate() {
                            (row.render_row(index, self.zone_for(row), self.now))
                        }
                    }
                }
                div class="empty" id="filtered-empty" hidden {
                    p class="empty-title" { "No matches" }
                    button type="button" class="btn btn-secondary" id="reset-filters-btn" { "Reset filters" }
                }
                @for (index, row) in self.rows.iter().enumerate() {
                    (row.render_detail(index, self.zone_for(row), self.now))
                }
            }
        }
    }

    fn render_footer(&self) -> Markup {
        html! {
            footer class="footer" {
                nav class="footer-links" aria-label="Links" {
                    a href="https://github.com/v3xlabs/dmn" target="_blank" rel="noreferrer" { "v3xlabs / dmn" }
                    a href="/api/rss.xml" target="_blank" rel="noreferrer" { "RSS" }
                    a href="/api/domains.ics" download="domains.ics" { "Calendar" }
                    a href="/docs" target="_blank" rel="noreferrer" { "OpenAPI" }
                }
                span class="mono" { "v" (env!("CARGO_PKG_VERSION")) }
            }
        }
    }

    fn render_drawer(&self) -> Markup {
        html! {
            dialog class="drawer" id="detail-drawer" aria-labelledby="drawer-title" {
                div class="drawer-controls" {
                    button type="button" class="glyph-btn has-tip" id="drawer-prev-btn" aria-label="Previous domain" {
                        (icons::ARROW_UP)
                        span class="tip" { "Previous domain" }
                    }
                    button type="button" class="glyph-btn has-tip" id="drawer-next-btn" aria-label="Next domain" {
                        (icons::ARROW_DOWN)
                        span class="tip" { "Next domain" }
                    }
                    button type="button" class="glyph-btn has-tip" id="drawer-close-btn" aria-label="Close" {
                        (icons::CLOSE)
                        span class="tip" { "Close" }
                    }
                }
                div class="drawer-content" id="drawer-content" {}
            }
        }
    }
}

fn totals_cell(filter: &str, label: &str, tone: Option<&str>, count: usize) -> Markup {
    html! {
        button type="button" class="totals-cell" data-status=(filter) aria-pressed=(filter == "all") {
            span class="totals-label" {
                @if let Some(tone) = tone {
                    span class="status-dot" data-tone=(tone) aria-hidden="true" {}
                }
                (label)
            }
            span class="totals-value tabular" { (count) }
        }
    }
}

fn provider_option(
    value: &str,
    icon: PreEscaped<&str>,
    label: &str,
    count: usize,
    selected: bool,
) -> Markup {
    html! {
        button type="button" class="provider-option" role="option" data-provider=(value) aria-selected=(selected) {
            span class="provider-icon" aria-hidden="true" { (icon) }
            span class="provider-option-label" { (label) }
            span class="provider-count tabular" { (count) }
            (icons::CHECK)
        }
    }
}

fn sort_header(key: &str, label: &str, sort: &str) -> Markup {
    html! {
        th scope="col" class={ "col-" (key) } aria-sort=(sort) {
            button type="button" class="th-sort" data-sort=(key) {
                (label)
                span class="sort-indicator" aria-hidden="true" {
                    span class="sort-idle" { (icons::SORT) }
                    span class="sort-asc" { (icons::ARROW_UP) }
                    span class="sort-desc" { (icons::ARROW_DOWN) }
                }
            }
        }
    }
}

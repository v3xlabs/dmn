use std::cmp::Reverse;

use chrono::{DateTime, Utc};
use maud::{html, Markup};

use crate::models::domain::Domain;

use super::details::Details;
use super::expiry::{relative_past, Expiry};
use super::history::Change;
use super::icons;
use super::provider::ProviderKind;
use super::registry::RegistryPanel;
use super::zone::CloudflareZone;

#[derive(Debug)]
pub struct DomainRow {
    pub domain: Domain,
    pub provider: ProviderKind,
    pub details: Details,
    pub registry: Option<RegistryPanel>,
    pub expiry: Expiry,
    pub changes: Vec<Change>,
}

impl DomainRow {
    pub fn new(
        domain: Domain,
        mut changes: Vec<Change>,
        registry: Option<RegistryPanel>,
        now: DateTime<Utc>,
    ) -> Self {
        let provider: ProviderKind = domain
            .provider
            .parse()
            .expect("ProviderKind has a default variant, so parsing cannot fail");
        let details = Details::new(&provider, domain.metadata.as_ref(), &domain.name);
        let expiry = Expiry::new(domain.ext_expiry_at, details.missing_expiry(), now);
        changes.sort_by_key(|change| Reverse(change.at));
        Self {
            domain,
            provider,
            details,
            registry,
            expiry,
            changes,
        }
    }

    pub fn synced_at(&self) -> Option<DateTime<Utc>> {
        self.domain.updated_at.or(self.domain.created_at)
    }

    fn search_text(&self) -> String {
        [
            self.domain.name.as_str(),
            &self.provider.to_string(),
            self.details.registrar().unwrap_or_default(),
            &self.expiry.label,
            &self.details.nameservers().join(" "),
        ]
        .join(" ")
        .to_lowercase()
    }

    pub fn render_row(
        &self,
        index: usize,
        zone: Option<&CloudflareZone>,
        now: DateTime<Utc>,
    ) -> Markup {
        let latest_change = self.changes.first();
        let renew = self.domain.ext_auto_renew.map(|on| match on {
            true => ("ok", "Auto-renew on"),
            false => ("warn", "Auto-renew off"),
        });
        let privacy = self.domain.ext_whois_privacy.map(|on| match on {
            true => ("ok", "WHOIS privacy on", icons::PRIVACY_ON),
            false => ("warn", "WHOIS details public", icons::PRIVACY_EXPOSED),
        });

        html! {
            tr
                class="domain-row"
                tabindex="0"
                data-index=(index)
                data-provider=(self.provider)
                data-name=(self.domain.name)
                data-search=(self.search_text())
                data-changed=(latest_change.map_or(0, |change| change.at.timestamp()))
                data-attention[self.expiry.needs_attention]
                data-window[self.expiry.within_window]
            {
                td {
                    span class="expiry" {
                        (self.expiry_line())
                        span class="expiry-tip" role="tooltip" { (self.expiry.hover) }
                    }
                }
                td {
                    div class="domain-line" {
                        div class="domain-name" { (domain_name(&self.domain.name)) }
                        @if zone.is_some() {
                            span class="zone-hint has-tip" role="img" aria-label="Cloudflare zone" {
                                (icons::CLOUDFLARE)
                                span class="tip" { "Cloudflare zone" }
                            }
                        }
                    }
                    div class="provider-inline" { (self.provider_mark()) }
                }
                td { (self.provider_mark()) }
                td {
                    div class="flags" {
                        @if let Some((state, title)) = renew {
                            span class="flag" role="img" data-state=(state) title=(title) aria-label=(title) { (icons::RENEW) }
                        }
                        @if let Some((state, title, icon)) = privacy {
                            span class="flag" role="img" data-state=(state) title=(title) aria-label=(title) { (icon) }
                        }
                    }
                }
                td {
                    div class="activity" {
                        @if let Some(change) = latest_change {
                            span class="activity-changed tabular" { (change.event.verb()) " " (relative_past(change.at, now)) }
                        } @else if let Some(synced) = self.synced_at() {
                            span class="tabular" { "Synced " (relative_past(synced, now)) }
                        }
                        (icons::CHEVRON_RIGHT)
                    }
                }
            }
        }
    }

    pub fn expiry_line(&self) -> Markup {
        html! {
            span class="status-line tabular" data-tone=(self.expiry.tone) {
                span class="status-dot" aria-hidden="true" {}
                span { (self.expiry.label) }
            }
        }
    }

    pub fn provider_mark(&self) -> Markup {
        html! {
            span class="provider-mark" {
                span class="provider-icon" aria-hidden="true" { (self.provider.icon()) }
                span class="provider-label" { (self.provider) }
            }
        }
    }
}

pub fn domain_name(name: &str) -> Markup {
    let (head, tail) = name.split_at(name.find('.').unwrap_or(name.len()));
    html! {
        span class="domain-sld" { (head) }
        span class="domain-tld" { (tail) }
    }
}

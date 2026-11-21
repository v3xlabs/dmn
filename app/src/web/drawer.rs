use chrono::{DateTime, Utc};
use maud::{html, Markup};

use super::expiry::{format_utc, relative_past, ExpiryTone};
use super::history::{Change, ChangeLine};
use super::icons;
use super::row::{domain_name, DomainRow};

impl DomainRow {
    pub fn render_detail(&self, index: usize, now: DateTime<Utc>) -> Markup {
        html! {
            template id={ "detail-" (index) } {
                header class="drawer-head" {
                    h2 class="drawer-domain" id="drawer-title" { (domain_name(&self.domain.name)) }
                    div class="drawer-meta" {
                        (self.provider_mark())
                        (self.expiry_line())
                        @if let Some(url) = self.details.manage_url(&self.domain.name) {
                            a class="drawer-link" href=(url) target="_blank" rel="noreferrer" {
                                "Manage" (icons::EXTERNAL)
                            }
                        }
                    }
                }
                div class="drawer-body" {
                    (self.render_lifecycle(now))
                    (self.render_registrar())
                    (self.render_history())
                }
            }
        }
    }

    fn render_lifecycle(&self, now: DateTime<Utc>) -> Markup {
        let domain = &self.domain;
        let expires = match domain.ext_expiry_at {
            Some(at) => format_utc(at),
            None if self.expiry.tone == ExpiryTone::Never => "Never".to_string(),
            None => "Unknown".to_string(),
        };
        let switches = [
            ("Auto-renew", domain.ext_auto_renew),
            ("WHOIS privacy", domain.ext_whois_privacy),
            ("Transfer lock", self.details.transfer_lock()),
        ];
        html! {
            section class="detail-section" {
                div class="detail-head" {
                    h3 class="detail-title" { "Lifecycle" }
                    @if let Some(synced) = self.synced_at() {
                        span class="detail-meta tabular" { "Synced " (relative_past(synced, now)) }
                    }
                }
                dl class="kv-grid" {
                    (kv("Status", &self.expiry.status, false))
                    (kv("Expires", &expires, true))
                    @if let Some(registered) = domain.ext_registered_at {
                        (kv("Registered", &format_utc(registered), true))
                    }
                    @for (label, value) in switches {
                        @if let Some(on) = value {
                            (kv(label, if on { "On" } else { "Off" }, false))
                        }
                    }
                }
            }
        }
    }

    fn render_registrar(&self) -> Markup {
        let fields = self.details.fields();
        let nameservers = self.details.nameservers();
        html! {
            @if !fields.is_empty() || !nameservers.is_empty() {
                section class="detail-section" {
                    div class="detail-head" {
                        h3 class="detail-title" { "Registration" }
                    }
                    @if !fields.is_empty() {
                        dl class="kv-grid" {
                            @for field in &fields {
                                (kv(field.label, field.value, field.mono))
                            }
                        }
                    }
                    @if !nameservers.is_empty() {
                        ul class="record-list" aria-label="Nameservers" {
                            @for nameserver in nameservers {
                                li class="record-row" {
                                    span class="record-type" { "NS" }
                                    span class="record-value mono" { (nameserver) }
                                    button type="button" class="copy-btn has-tip" data-copy=(nameserver) aria-label="Copy" {
                                        (icons::COPY)
                                        span class="tip" { "Copy" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    fn render_history(&self) -> Markup {
        html! {
            section class="detail-section" {
                div class="detail-head" {
                    h3 class="detail-title" { "Change history" }
                }
                @if self.changes.is_empty() {
                    p class="detail-empty" { "No changes recorded yet." }
                } @else {
                    ol class="timeline" {
                        @for change in &self.changes {
                            (change.render())
                        }
                    }
                }
            }
        }
    }
}

impl Change {
    fn render(&self) -> Markup {
        html! {
            li class="timeline-item" data-event=(self.event) {
                div class="timeline-head" {
                    span class="timeline-title" { (self.event.title()) }
                    time class="timeline-time tabular" datetime=(self.at.to_rfc3339()) { (format_utc(self.at)) }
                }
                @if self.lines.iter().any(|line| matches!(line, ChangeLine::Field { .. })) {
                    div class="diff mono" {
                        @for line in &self.lines {
                            @match line {
                                ChangeLine::Field { field, before, after } => {
                                    div class="diff-line" data-kind="del" { "- " (field) ": " (before) }
                                    div class="diff-line" data-kind="add" { "+ " (field) ": " (after) }
                                }
                                ChangeLine::Note(note) => {
                                    div class="diff-line" data-kind="note" { (note) }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn kv(label: &str, value: &str, mono: bool) -> Markup {
    html! {
        div class="kv" {
            dt class="kv-label" { (label) }
            dd class={ "kv-value" @if mono { " mono" } } { (value) }
        }
    }
}

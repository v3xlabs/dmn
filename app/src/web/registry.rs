use std::collections::HashMap;

use chrono::{DateTime, Utc};
use maud::{html, Markup};
use serde::Deserialize;
use serde_json::Value;
use strum::EnumString;
use tracing::warn;

use crate::{
    models::{
        index_state::{IndexKind, IndexState},
        record_change::LoggedChange,
        snapshot::SnapshotSummary,
    },
    modules::registry::record::RegistryRecord,
    state::AppState,
};

use super::{
    drawer::kv,
    expiry::{format_utc, relative_past},
    history::Change,
    icons,
    provider::ProviderKind,
    row::DomainRow,
};

#[derive(Debug, Clone)]
pub struct RegistryPanel {
    pub latest: Option<(SnapshotSummary, RegistryRecord)>,
    pub last_error: Option<String>,
}

/// A field of `RegistryRecord`, by the name it has in `snapshots.fields` and `record_changes.field`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumString)]
#[strum(serialize_all = "snake_case")]
pub enum RegistryField {
    Registered,
    ExpiryAt,
    RegisteredAt,
    Statuses,
    Nameservers,
    Registrar,
    Dnssec,
}

impl RegistryField {
    pub fn label(self) -> &'static str {
        match self {
            Self::Registered => "Registered",
            Self::ExpiryAt => "Expires",
            Self::RegisteredAt => "Created",
            Self::Statuses => "Status",
            Self::Nameservers => "Nameservers",
            Self::Registrar => "Registrar",
            Self::Dnssec => "DNSSEC",
        }
    }

    pub fn format(self, value: &Value) -> String {
        match (self, value) {
            (_, Value::Null) => "None".to_string(),
            (Self::ExpiryAt | Self::RegisteredAt, Value::String(text)) => {
                DateTime::parse_from_rfc3339(text)
                    .map(|at| format_utc(at.with_timezone(&Utc)))
                    .unwrap_or_else(|_| text.clone())
            }
            (Self::Dnssec, Value::Bool(signed)) => dnssec_label(*signed).to_string(),
            (_, Value::Bool(true)) => "Yes".to_string(),
            (_, Value::Bool(false)) => "No".to_string(),
            (_, Value::Array(items)) => items
                .iter()
                .map(|item| {
                    item.as_str()
                        .map_or_else(|| item.to_string(), str::to_string)
                })
                .collect::<Vec<_>>()
                .join(", "),
            (_, Value::String(text)) => text.clone(),
            (_, other) => other.to_string(),
        }
    }
}

fn dnssec_label(signed: bool) -> &'static str {
    if signed {
        "Signed"
    } else {
        "Unsigned"
    }
}

pub struct RegistryData {
    pub panels: HashMap<String, RegistryPanel>,
    pub changes: HashMap<String, Vec<Change>>,
}

impl RegistryData {
    /// `history_limit` caps the registry snapshots read per name, like the notification history caps entries.
    pub async fn load(state: &AppState, history_limit: u32) -> Result<Self, sqlx::Error> {
        let mut panels: HashMap<String, RegistryPanel> =
            IndexState::all(state, IndexKind::Registry)
                .await?
                .into_iter()
                .map(|row| {
                    let panel = RegistryPanel {
                        latest: None,
                        last_error: row.last_error,
                    };
                    (row.name, panel)
                })
                .collect();
        for summary in SnapshotSummary::latest_per_name(state, IndexKind::Registry).await? {
            let Some(panel) = panels.get_mut(&summary.name) else {
                continue;
            };
            match RegistryRecord::deserialize(&summary.fields.0) {
                Ok(record) => panel.latest = Some((summary, record)),
                Err(error) => {
                    warn!(domain.name = %summary.name, %error, "stored registry fields do not match RegistryRecord");
                }
            }
        }

        let mut conn = state.database.pool.acquire().await?;
        let logged =
            LoggedChange::latest_per_name(&mut conn, IndexKind::Registry, history_limit).await?;
        let mut changes: HashMap<String, Vec<Change>> = HashMap::new();
        for snapshot in logged.chunk_by(|a, b| a.snapshot_id == b.snapshot_id) {
            changes
                .entry(snapshot[0].name.clone())
                .or_default()
                .push(Change::registry(snapshot));
        }

        Ok(Self { panels, changes })
    }
}

impl DomainRow {
    pub fn render_registry(&self, now: DateTime<Utc>) -> Markup {
        let Some(panel) = &self.registry else {
            return html! {};
        };
        // Lifecycle already shows the registry dates of a manual row.
        let show_dates = self.provider != ProviderKind::Manual;
        html! {
            section class="detail-section" {
                div class="detail-head" {
                    h3 class="detail-title" { "Registry" }
                    @if let Some((summary, _)) = &panel.latest {
                        div class="detail-actions" {
                            span class="detail-meta tabular" {
                                (summary.source.to_string().to_uppercase()) ", checked " (relative_past(summary.confirmed_at, now))
                            }
                            button type="button" class="glyph-btn has-tip raw-btn" data-name=(self.domain.name) data-snapshot=(summary.id) aria-expanded="false" aria-label="Raw response" {
                                (icons::CODE)
                                span class="tip" { "Raw response" }
                            }
                        }
                    }
                }
                @match &panel.latest {
                    None => {
                        p class="detail-empty" {
                            @match &panel.last_error {
                                Some(error) => { "Lookup failed: " (error) }
                                None => { "Not checked yet" }
                            }
                        }
                    }
                    Some((summary, record)) => {
                        div class="raw-panel" hidden {
                            p class="detail-meta raw-server mono" { (summary.server) }
                            pre class="raw-text mono" {}
                            p class="detail-empty" role="alert" hidden { "Unavailable" }
                        }
                        (record.render(show_dates))
                        @if let Some(error) = &panel.last_error {
                            p class="detail-empty registry-error" { "Last lookup failed: " (error) }
                        }
                    }
                }
            }
        }
    }
}

impl RegistryRecord {
    fn render(&self, show_dates: bool) -> Markup {
        if !self.registered {
            return html! {
                @if show_dates {
                    dl class="kv-grid" {
                        (kv(RegistryField::Registered.label(), "No", false))
                    }
                }
            };
        }
        let expires = self
            .expiry_at
            .map_or_else(|| "Not published".to_string(), format_utc);
        html! {
            dl class="kv-grid" {
                @if show_dates {
                    (kv(RegistryField::ExpiryAt.label(), &expires, true))
                    @if let Some(created) = self.registered_at {
                        (kv(RegistryField::RegisteredAt.label(), &format_utc(created), true))
                    }
                }
                @if let Some(registrar) = &self.registrar {
                    (kv(RegistryField::Registrar.label(), registrar, false))
                }
                @if let Some(signed) = self.dnssec {
                    (kv(RegistryField::Dnssec.label(), dnssec_label(signed), false))
                }
                @if let Some(statuses) = self.statuses.as_ref().filter(|list| !list.is_empty()) {
                    div class="kv kv-wide" {
                        dt class="kv-label" { (RegistryField::Statuses.label()) }
                        dd class="kv-value" {
                            @for status in statuses {
                                div { (status) }
                            }
                        }
                    }
                }
            }
            @if let Some(nameservers) = self.nameservers.as_ref().filter(|list| !list.is_empty()) {
                ul class="record-list" aria-label=(RegistryField::Nameservers.label()) {
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

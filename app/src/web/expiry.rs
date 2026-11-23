use chrono::{DateTime, Utc};
use strum::Display;

/// Renewals inside this many days count under Attention; registrars start sending renewal reminders around this point.
const SOON_DAYS: i64 = 30;
/// Renewals inside this many days count under the "Within 90d" filter, matching its label.
const WINDOW_DAYS: i64 = 90;
/// Below this many months the label stays in months, so "in 14 months" does not round to "in 1 year".
const YEAR_LABEL_MONTHS: i64 = 18;
/// Mean Gregorian month length, used only for coarse relative labels.
const DAYS_PER_MONTH: f64 = 30.44;
/// Mean Gregorian year length, used only for coarse relative labels.
const DAYS_PER_YEAR: f64 = 365.25;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
#[strum(serialize_all = "lowercase")]
pub enum ExpiryTone {
    Unknown,
    Expired,
    Soon,
    Active,
    Never,
    Unpublished,
    Pending,
}

/// What a domain row without `ext_expiry_at` means, which depends on where the row comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MissingExpiry {
    Never,
    Unpublished,
    NotChecked,
    Unregistered,
    LookupFailed(String),
    Unreported,
}

#[derive(Debug, Clone)]
pub struct Expiry {
    pub tone: ExpiryTone,
    pub label: String,
    pub hover: String,
    pub status: String,
    pub needs_attention: bool,
    pub within_window: bool,
}

impl Expiry {
    pub fn new(expiry: Option<DateTime<Utc>>, missing: MissingExpiry, now: DateTime<Utc>) -> Self {
        let Some(expiry) = expiry else {
            let (tone, label, hover, status, needs_attention) = match missing {
                MissingExpiry::Never => (
                    ExpiryTone::Never,
                    "No expiry",
                    "This name does not expire".to_string(),
                    "No expiry".to_string(),
                    false,
                ),
                MissingExpiry::Unpublished => (
                    ExpiryTone::Unpublished,
                    "Not published",
                    "No expiry published".to_string(),
                    "Registered".to_string(),
                    false,
                ),
                MissingExpiry::NotChecked => (
                    ExpiryTone::Pending,
                    "Not checked yet",
                    "Not looked up yet".to_string(),
                    "Not checked yet".to_string(),
                    false,
                ),
                MissingExpiry::Unregistered => (
                    ExpiryTone::Unknown,
                    "Not registered",
                    "No registry record".to_string(),
                    "Not registered".to_string(),
                    true,
                ),
                MissingExpiry::LookupFailed(error) => (
                    ExpiryTone::Unknown,
                    "Lookup failed",
                    error.clone(),
                    // The drawer shows `status` as plain text, so the error is readable without hover.
                    format!("Lookup failed: {error}"),
                    true,
                ),
                MissingExpiry::Unreported => (
                    ExpiryTone::Unknown,
                    "Expiry unknown",
                    "The provider reported no expiry date".to_string(),
                    "Expiry unknown".to_string(),
                    true,
                ),
            };
            return Self {
                tone,
                label: label.to_string(),
                hover,
                status,
                needs_attention,
                within_window: false,
            };
        };

        let days =
            (expiry.signed_duration_since(now).num_seconds() as f64 / 86_400.0).round() as i64;
        let hover = format_utc(expiry);

        if days < 0 {
            let label = format!("{}d overdue", days.unsigned_abs());
            return Self {
                tone: ExpiryTone::Expired,
                status: format!("Expired ({label})"),
                label,
                hover,
                needs_attention: true,
                within_window: false,
            };
        }

        let label = relative_future(days);
        let (tone, status) = if days <= SOON_DAYS {
            (ExpiryTone::Soon, format!("Expiring soon ({label})"))
        } else {
            (ExpiryTone::Active, format!("Active ({label})"))
        };

        Self {
            tone,
            status,
            label,
            hover,
            needs_attention: days <= SOON_DAYS,
            within_window: days <= WINDOW_DAYS,
        }
    }
}

pub fn format_utc(at: DateTime<Utc>) -> String {
    at.format("%Y-%m-%d %H:%M UTC").to_string()
}

pub fn relative_past(at: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let minutes = now.signed_duration_since(at).num_minutes().max(0);
    match minutes {
        0 => "just now".to_string(),
        1..60 => format!("{minutes}m ago"),
        60..2880 => format!("{}h ago", minutes / 60),
        _ => format!("{}d ago", minutes / 1440),
    }
}

fn relative_future(days: i64) -> String {
    if days == 0 {
        return "today".to_string();
    }
    if days == 1 {
        return "in 1 day".to_string();
    }
    if days <= WINDOW_DAYS {
        return format!("in {days} days");
    }
    let months = (days as f64 / DAYS_PER_MONTH).round() as i64;
    if months < YEAR_LABEL_MONTHS {
        return format!("in {months} months");
    }
    let years = (days as f64 / DAYS_PER_YEAR).round() as i64;
    format!("in {years} years")
}

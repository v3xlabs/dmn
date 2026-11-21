use std::time::Duration as StdDuration;

use async_std::task;
use chrono::{DateTime, Duration, Utc};
use tracing::{error, info};

use super::{EnsName, EnsService};
use crate::{models::domain::Domain, modules::domains::diff_provider, state::AppState};

/// Checks ENS domains weekly because on-chain `expires_at` only moves forward on renewal, avoiding unnecessary public RPC traffic.
const ENS_BASELINE_REFRESH_DAYS: i64 = 7;

/// Re-checks at 30, 15, 7, 3, and 1 day before `ext_expiry_at` so renewals and imminent expiries surface promptly without frequent polling.
const ENS_EXPIRY_CHECKPOINT_DAYS: &[i64] = &[30, 15, 7, 3, 1];

/// Checks once per day while an expired domain sits inside its 28-day or 90-day renewal grace window so renewals or releases are detected within 24 hours.
const ENS_GRACE_REFRESH_DAYS: i64 = 1;

/// Waits one hour after an RPC or database error so transient endpoint outages do not cause tight retry loops against public RPCs.
const ENS_ERROR_RETRY_SECS: u64 = 60 * 60;

#[derive(Default)]
pub struct RefreshSchedule {
    last_sync_at: Option<DateTime<Utc>>,
}

impl RefreshSchedule {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn mark_synced(&mut self, at: DateTime<Utc>) {
        self.last_sync_at = Some(at);
    }

    pub fn next_refresh_at(
        &self,
        stored_domains: &[Domain],
        configured_names: &[EnsName],
        now: DateTime<Utc>,
    ) -> DateTime<Utc> {
        let baseline_due =
            self.last_sync_at.unwrap_or(now) + Duration::days(ENS_BASELINE_REFRESH_DAYS);

        if stored_domains.is_empty() {
            return if self.last_sync_at.is_some() {
                baseline_due.max(now)
            } else {
                now
            };
        }

        let has_missing = configured_names.iter().any(|configured| {
            !stored_domains
                .iter()
                .any(|d| d.name.eq_ignore_ascii_case(configured.as_str()))
        });
        if has_missing && self.last_sync_at.is_none() {
            return now;
        }

        let mut earliest = baseline_due;
        for domain in stored_domains {
            let due = self.domain_refresh_at(domain, now);
            if due <= now {
                return now;
            }
            if due < earliest {
                earliest = due;
            }
        }
        earliest.max(now)
    }

    fn domain_refresh_at(&self, domain: &Domain, now: DateTime<Utc>) -> DateTime<Utc> {
        let last_synced = match (domain.updated_at, self.last_sync_at) {
            (Some(updated), Some(synced)) => updated.max(synced),
            (Some(updated), None) => updated,
            (None, Some(synced)) => synced,
            (None, None) => return now,
        };

        let mut due = last_synced + Duration::days(ENS_BASELINE_REFRESH_DAYS);

        if let Some(expiry) = domain.ext_expiry_at {
            for &days_before in ENS_EXPIRY_CHECKPOINT_DAYS {
                let checkpoint = expiry - Duration::days(days_before);
                if checkpoint > last_synced && checkpoint < due {
                    due = checkpoint;
                }
            }
            if expiry > last_synced && expiry < due {
                due = expiry;
            }

            let grace_ends = domain
                .metadata
                .as_ref()
                .and_then(|meta| meta.get("grace_ends_at"))
                .and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc));

            if expiry <= now && grace_ends.is_some_and(|grace| now < grace) {
                let daily = last_synced + Duration::days(ENS_GRACE_REFRESH_DAYS);
                if daily < due {
                    due = daily;
                }
            }
        }

        due
    }
}

impl EnsService {
    pub async fn run_schedule(&self, state: &AppState) {
        let mut schedule = RefreshSchedule::new();

        loop {
            let stored = match Domain::find_by_provider(state, "ens").await {
                Ok(domains) => domains,
                Err(err) => {
                    error!(error = %err, "failed to load stored ENS domains for schedule");
                    task::sleep(StdDuration::from_secs(ENS_ERROR_RETRY_SECS)).await;
                    continue;
                }
            };

            let now = Utc::now();
            let next_at = schedule.next_refresh_at(&stored, &self.config.names, now);
            if next_at > now {
                let wait_secs = (next_at - now).num_seconds().max(1) as u64;
                info!(
                    ens.next_refresh_at = %next_at.to_rfc3339(),
                    ens.wait_secs = wait_secs,
                    "sleeping until next adaptive ENS refresh"
                );
                task::sleep(StdDuration::from_secs(wait_secs)).await;
                continue;
            }

            match diff_provider(state, "ens", self).await {
                Ok(_) => schedule.mark_synced(Utc::now()),
                Err(err) => {
                    error!(error = %err, "adaptive ENS refresh failed");
                    task::sleep(StdDuration::from_secs(ENS_ERROR_RETRY_SECS)).await;
                }
            }
        }
    }
}

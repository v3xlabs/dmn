use std::time::Duration as StdDuration;

use async_std::task;
use chrono::{DateTime, Duration, Utc};
use tracing::{error, info};

use super::{domains::Discovery, EnsDomain, EnsService};
use crate::{
    models::index_state::{IndexKind, IndexState},
    state::AppState,
};

/// Resolves a name far from expiry once a month.
/// On-chain expiry only moves later, and the monthly lookup catches owner changes and corrects a stale stored expiry.
const ENS_BASELINE_REFRESH_DAYS: i64 = 30;

/// Re-checks at 30, 15, 7, 3, and 1 day before expiry so renewals and imminent expiries surface promptly without frequent polling.
const ENS_EXPIRY_CHECKPOINT_DAYS: &[i64] = &[30, 15, 7, 3, 1];

/// Checks once per day while an expired domain sits inside its 28-day or 90-day renewal grace window so renewals or releases are detected within 24 hours.
const ENS_GRACE_REFRESH_DAYS: i64 = 1;

/// Waits a week before looking up a name that resolved to nothing (unregistered, mistyped, or past grace) again.
/// The wait lives in `index_state`, so a restart loop does not query the RPC on every boot.
pub const ENS_UNRESOLVED_RECHECK_DAYS: i64 = 7;

/// Waits one hour after a failed lookup or a database error, so transient endpoint outages do not cause tight retry loops against public RPCs.
pub const ENS_ERROR_RETRY_SECS: u64 = 60 * 60;

/// Asks discovery for the names of each address once a day, so a name an address gains is tracked within a day.
/// A failed discovery keeps the last listed names and waits the same day.
pub const ENS_DISCOVERY_INTERVAL_HOURS: i64 = 24;

impl EnsDomain {
    pub fn next_check_at(&self, now: DateTime<Utc>) -> DateTime<Utc> {
        let expiry = self.expiry_date;
        let checkpoints = ENS_EXPIRY_CHECKPOINT_DAYS
            .iter()
            .map(|days| expiry - Duration::days(*days))
            .chain([expiry]);
        let mut due = now + Duration::days(ENS_BASELINE_REFRESH_DAYS);
        for checkpoint in checkpoints {
            if checkpoint > now && checkpoint < due {
                due = checkpoint;
            }
        }
        let in_grace = self
            .metadata
            .grace_ends_at
            .is_some_and(|grace_ends| expiry <= now && now < grace_ends);
        if in_grace {
            due = due.min(now + Duration::days(ENS_GRACE_REFRESH_DAYS));
        }
        due
    }
}

impl EnsService {
    pub async fn run_schedule(&self, state: &AppState) {
        let mut discovery = Discovery::default();

        loop {
            let now = Utc::now();
            let discovery_at = discovery.next_at(now);
            let next_at = match IndexState::earliest_check_at(state, IndexKind::Ens).await {
                Ok(earliest) => earliest.map_or(discovery_at, |at| at.min(discovery_at)),
                Err(err) => {
                    error!(error = %err, "failed to load ENS due times for schedule");
                    task::sleep(StdDuration::from_secs(ENS_ERROR_RETRY_SECS)).await;
                    continue;
                }
            };
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

            if let Err(err) = self.refresh(state, &mut discovery, false).await {
                error!(error = %format_args!("{err:#}"), "adaptive ENS refresh failed");
                task::sleep(StdDuration::from_secs(ENS_ERROR_RETRY_SECS)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, sync::Arc};

    use chrono::{SubsecRound, TimeZone};
    use figment::Figment;
    use sqlx::sqlite::SqlitePoolOptions;

    use super::*;
    use crate::{
        cache::AppCache,
        database::Database,
        modules::ens::{domains::Discovery, EnsConfig, EnsMetadata, EnsVersion},
        state::{AppStateInner, LoadedConfig},
    };

    /// Nothing listens on the discard port, so every RPC and discovery request fails at once and no test needs the network.
    const UNREACHABLE: &str = "http://127.0.0.1:9";

    const ADDRESS: &str = "0xd0b1a7438002db376ee2e9f29a036d3c858057dd";

    async fn memory_state() -> AppState {
        // One connection that never closes, because every connection to `sqlite::memory:` opens its own empty database.
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .idle_timeout(None)
            .max_lifetime(None)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let database = Database { pool };
        database.migrate().await;
        Arc::new(AppStateInner {
            database,
            api: None,
            cache: AppCache::new(),
            porkbun: None,
            cloudflare: None,
            ens: None,
            manual: None,
            ntfy: None,
            config: LoadedConfig {
                figment: Figment::new(),
                path: PathBuf::new(),
                deletes: None,
            },
        })
    }

    fn unreachable_service(names: &[&str], addresses: &[&str]) -> EnsService {
        EnsService::new(EnsConfig {
            names: names.iter().map(|name| name.parse().unwrap()).collect(),
            addresses: addresses
                .iter()
                .map(|address| address.parse().unwrap())
                .collect(),
            rpc_url: Some(UNREACHABLE.into()),
            #[cfg(feature = "ens-discovery")]
            discovery_url: Some(UNREACHABLE.into()),
        })
    }

    async fn seed(state: &AppState, name: &str, next_check_at: DateTime<Utc>, failures: i64) {
        let mut conn = state.database.pool.acquire().await.unwrap();
        IndexState::add(&mut conn, name, IndexKind::Ens, next_check_at)
            .await
            .unwrap();
        sqlx::query("UPDATE index_state SET failures = ? WHERE name = ?")
            .bind(failures)
            .bind(name)
            .execute(&mut *conn)
            .await
            .unwrap();
    }

    async fn row(state: &AppState, name: &str) -> Option<IndexState> {
        sqlx::query_as("SELECT * FROM index_state WHERE name = ? AND kind = 'ens'")
            .bind(name)
            .fetch_optional(&state.database.pool)
            .await
            .unwrap()
    }

    #[async_std::test]
    async fn retry_without_stored_domain_survives_restart_with_discovery_down() {
        let state = memory_state().await;
        let now = Utc::now();
        seed(&state, "payphone.eth", now - Duration::minutes(1), 1).await;
        let ens = unreachable_service(&[], &[ADDRESS]);

        let lookups = ens
            .refresh(&state, &mut Discovery::default(), false)
            .await
            .unwrap();

        assert_eq!(lookups.total, 1);
        let retry = row(&state, "payphone.eth")
            .await
            .expect("a failed retry keeps its row");
        assert_eq!(retry.failures, 2);
        assert!(retry.next_check_at > now, "{}", retry.next_check_at);
    }

    #[async_std::test]
    async fn refresh_looks_up_only_due_names_unless_all_names() {
        let state = memory_state().await;
        let now = Utc::now();
        let later = (now + Duration::days(10)).trunc_subsecs(0);
        seed(&state, "nick.eth", now - Duration::minutes(1), 0).await;
        seed(&state, "dialing.eth", later, 0).await;
        let ens = unreachable_service(&["nick.eth", "dialing.eth"], &[]);

        let due_only = ens
            .refresh(&state, &mut Discovery::default(), false)
            .await
            .unwrap();
        assert_eq!(due_only.total, 1);
        assert_eq!(row(&state, "nick.eth").await.unwrap().failures, 1);
        let not_due = row(&state, "dialing.eth").await.unwrap();
        assert_eq!((not_due.failures, not_due.next_check_at), (0, later));

        let all_names = ens
            .refresh(&state, &mut Discovery::default(), true)
            .await
            .unwrap();
        assert_eq!(all_names.total, 2);
    }

    fn expiring(expiry: DateTime<Utc>, grace_ends_at: Option<DateTime<Utc>>) -> EnsDomain {
        EnsDomain {
            name: "example.eth".into(),
            expiry_date: expiry,
            registration_date: None,
            metadata: EnsMetadata {
                status: "active".into(),
                ens_version: EnsVersion::V1,
                authority: None,
                namehash: None,
                owner: None,
                manager: None,
                grace_ends_at,
                v1_expiry_at: None,
                wrapper_state: None,
                resolver: None,
            },
        }
    }

    #[test]
    fn far_expiry_waits_the_monthly_baseline() {
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let due = expiring(now + Duration::days(400), None).next_check_at(now);

        assert_eq!(due, now + Duration::days(30));
    }

    #[test]
    fn near_expiry_hits_next_checkpoint_then_expiry() {
        let expiry = Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap();

        let forty_days_out = expiry - Duration::days(40);
        assert_eq!(
            expiring(expiry, None).next_check_at(forty_days_out),
            expiry - Duration::days(30)
        );

        let twelve_hours_out = expiry - Duration::hours(12);
        assert_eq!(
            expiring(expiry, None).next_check_at(twelve_hours_out),
            expiry
        );
    }

    #[test]
    fn expired_name_is_checked_daily_until_grace_ends() {
        let expiry = Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap();
        let grace_ends = expiry + Duration::days(90);

        let in_grace = expiry + Duration::days(40);
        assert_eq!(
            expiring(expiry, Some(grace_ends)).next_check_at(in_grace),
            in_grace + Duration::days(1)
        );

        let after_grace = grace_ends + Duration::days(1);
        assert_eq!(
            expiring(expiry, Some(grace_ends)).next_check_at(after_grace),
            after_grace + Duration::days(30)
        );
    }
}

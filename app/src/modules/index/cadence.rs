use std::hash::{DefaultHasher, Hash, Hasher};

use chrono::{DateTime, TimeDelta, Utc};

use crate::{models::index_state::IndexState, modules::registry::record::RegistryRecord};

/// Registry data rarely changes outside renewals, which the expiry checkpoints cover, so a weekly look is enough.
const REGISTRY_BASELINE_DAYS: i64 = 7;

/// Looks again this many days before `expiry_at`, so a renewal or a lapse shows up close to when it happens.
const REGISTRY_EXPIRY_CHECKPOINT_DAYS: &[i64] = &[60, 30, 14, 7, 3, 1];

/// The gTLD life cycle after expiry: up to 45 days auto-renew grace, 30 days redemption, 5 days pending delete.
/// Daily checks over that window catch a late renewal or the drop within a day.
const REGISTRY_POST_EXPIRY_DAYS: i64 = 80;

/// Each baseline due time moves later by up to this fraction of the interval, so names added together drift apart.
const JITTER_DIVISOR: i64 = 10;

/// First retry after a failed fetch; it doubles per consecutive failure.
const RETRY_BASE_SECS: i64 = 60 * 60;

/// Longest wait between retries, so a recovered registry is picked up within a day.
const RETRY_MAX_SECS: i64 = 24 * 60 * 60;

/// A TLD with no RDAP or WHOIS server can gain one in the IANA bootstrap; a monthly look is enough.
pub const NO_ROUTE_RECHECK_DAYS: i64 = 30;

impl RegistryRecord {
    pub fn next_check_at(&self, name: &str, now: DateTime<Utc>) -> DateTime<Utc> {
        let baseline = TimeDelta::days(REGISTRY_BASELINE_DAYS);
        let mut due = now + baseline + jitter(name, baseline);
        let Some(expiry) = self.expiry_at else {
            return due;
        };
        let checkpoints = REGISTRY_EXPIRY_CHECKPOINT_DAYS
            .iter()
            .map(|days| expiry - TimeDelta::days(*days))
            .chain([expiry]);
        for checkpoint in checkpoints {
            if checkpoint > now && checkpoint < due {
                due = checkpoint;
            }
        }
        if expiry <= now && now < expiry + TimeDelta::days(REGISTRY_POST_EXPIRY_DAYS) {
            due = due.min(now + TimeDelta::days(1));
        }
        due
    }
}

impl IndexState {
    /// The next try after a failed lookup. The wait doubles with every consecutive failure, this one included.
    pub fn retry_at(&self, now: DateTime<Utc>) -> DateTime<Utc> {
        let doublings = (self.failures + 1).clamp(1, 16) - 1;
        let secs = (RETRY_BASE_SECS << doublings).min(RETRY_MAX_SECS);
        now + TimeDelta::seconds(secs)
    }
}

/// Same offset for a name on every run of one binary, which is all the spreading needs.
fn jitter(name: &str, interval: TimeDelta) -> TimeDelta {
    let mut hasher = DefaultHasher::new();
    name.hash(&mut hasher);
    let span = (interval.num_seconds() / JITTER_DIVISOR).max(1);
    TimeDelta::seconds((hasher.finish() % span.unsigned_abs()) as i64)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use crate::models::index_state::IndexKind;

    use super::*;

    fn expiring(expiry: DateTime<Utc>) -> RegistryRecord {
        RegistryRecord {
            registered: true,
            expiry_at: Some(expiry),
            ..RegistryRecord::default()
        }
    }

    #[test]
    fn far_expiry_uses_jittered_baseline() {
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let due = expiring(now + TimeDelta::days(400)).next_check_at("example.com", now);

        assert!(due >= now + TimeDelta::days(7), "{due}");
        assert!(
            due < now + TimeDelta::days(7) + TimeDelta::hours(17),
            "{due}"
        );
    }

    #[test]
    fn near_expiry_hits_next_checkpoint_then_expiry() {
        let expiry = Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap();

        let ten_days_out = expiry - TimeDelta::days(10);
        assert_eq!(
            expiring(expiry).next_check_at("example.com", ten_days_out),
            expiry - TimeDelta::days(7)
        );

        let twelve_hours_out = expiry - TimeDelta::hours(12);
        assert_eq!(
            expiring(expiry).next_check_at("example.com", twelve_hours_out),
            expiry
        );
    }

    #[test]
    fn expired_name_is_checked_daily_until_drop_window_ends() {
        let expiry = Utc.with_ymd_and_hms(2026, 3, 1, 0, 0, 0).unwrap();

        let in_grace = expiry + TimeDelta::days(40);
        assert_eq!(
            expiring(expiry).next_check_at("example.com", in_grace),
            in_grace + TimeDelta::days(1)
        );

        let after_drop = expiry + TimeDelta::days(81);
        assert!(
            expiring(expiry).next_check_at("example.com", after_drop)
                >= after_drop + TimeDelta::days(7)
        );
    }

    #[test]
    fn retries_double_up_to_a_day() {
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let failed = |failures| IndexState {
            name: "example.com".into(),
            kind: IndexKind::Registry,
            next_check_at: now,
            last_checked_at: None,
            failures,
            last_error: None,
        };

        assert_eq!(failed(0).retry_at(now), now + TimeDelta::hours(1));
        assert_eq!(failed(2).retry_at(now), now + TimeDelta::hours(4));
        assert_eq!(failed(39).retry_at(now), now + TimeDelta::hours(24));
    }
}

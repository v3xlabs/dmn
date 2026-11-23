use std::{
    collections::HashMap,
    time::{Duration, Instant},
};

use anyhow::{anyhow, Error};
use async_std::task;
use chrono::{DateTime, TimeDelta, Utc};
use serde::Deserialize;
use tracing::{debug, error, info, warn};

use crate::{
    models::{
        domain::{Domain, ManualStatus},
        index_host::IndexHost,
        index_state::{IndexKind, IndexState},
        record_change::RecordChange,
        snapshot::{Observation, Snapshot},
    },
    modules::registry::{
        bootstrap::BootstrapUnavailable,
        name::DomainName,
        rdap::{Redirected, Throttled},
        record::RegistryRecord,
        IanaQuery, Lookup, Registry, Route, RouteStep, MAX_RDAP_REDIRECTS,
    },
    state::AppState,
};

pub mod cadence;

/// At most one lookup every 15 seconds across all servers, so even a first index of hundreds of names stays at 240 lookups per hour.
const INDEX_MIN_GAP_SECS: u64 = 15;

/// At most one lookup per minute against one RDAP or WHOIS host. Most names share a few registry hosts, such as Verisign for `.com`.
const HOST_MIN_GAP_SECS: i64 = 60;

/// How long a host that answered 429 or 503 without `Retry-After` is left alone.
const THROTTLE_PARK_SECS: i64 = 60 * 60;

/// How often `index_state` picks up names that providers added or dropped, and the longest idle sleep.
const RESYNC_SECS: u64 = 10 * 60;

/// Wait after a database error, or while no IANA bootstrap is available, before the loop tries again.
const ERROR_RETRY_SECS: u64 = 60 * 60;

/// Where an RDAP redirect sent a name, and how many hops it took to get there.
struct PendingRedirect {
    route: Route,
    hops: u8,
}

/// Fetches registry records for every tracked name, one at a time, and stores each change.
pub struct Indexer {
    registry: Registry,
    /// When each host may be queried again, mirrored in `index_hosts` so a restart keeps it.
    hosts: HashMap<String, DateTime<Utc>>,
    /// The next RDAP URL of a name whose last request was redirected. A restart starts again at the registry.
    redirects: HashMap<String, PendingRedirect>,
    synced_at: Option<Instant>,
}

impl Indexer {
    pub fn new() -> Self {
        Self {
            registry: Registry::new(),
            hosts: HashMap::new(),
            redirects: HashMap::new(),
            synced_at: None,
        }
    }

    pub async fn run(mut self, state: &AppState) {
        info!("registry indexer started");
        // The gap between two requests is not stored, so a restart waits it out before the first one.
        task::sleep(Duration::from_secs(INDEX_MIN_GAP_SECS)).await;
        loop {
            let wait = match self.step(state).await {
                Ok(wait) => wait,
                Err(err) => {
                    error!(error = %err, "registry index step failed");
                    Duration::from_secs(ERROR_RETRY_SECS)
                }
            };
            task::sleep(wait).await;
        }
    }

    /// Sends at most one request, to IANA or to a registry, and returns how long to sleep before the next step.
    async fn step(&mut self, state: &AppState) -> Result<Duration, Error> {
        if self
            .synced_at
            .is_none_or(|at| at.elapsed() >= Duration::from_secs(RESYNC_SECS))
        {
            if self.synced_at.is_none() {
                self.hosts = IndexHost::all(state)
                    .await?
                    .into_iter()
                    .map(|row| (row.host, row.free_at))
                    .collect();
            }
            IndexState::sync(state, IndexKind::Registry, Utc::now()).await?;
            self.synced_at = Some(Instant::now());
        }

        let now = Utc::now();
        let mut host_free_at: Option<DateTime<Utc>> = None;
        for row in IndexState::due(state, now).await? {
            let name: DomainName = match row.name.parse() {
                Ok(name) => name,
                Err(reason) => {
                    row.record_failure(
                        state,
                        now,
                        now + TimeDelta::days(cadence::NO_ROUTE_RECHECK_DAYS),
                        reason,
                    )
                    .await?;
                    continue;
                }
            };
            if let Some(redirect) = self.redirects.get(&row.name) {
                let host = redirect.route.host();
                if let Some(free_at) = self.parked_until(&host, now) {
                    host_free_at = Some(host_free_at.map_or(free_at, |at| at.min(free_at)));
                    continue;
                }
                let route = redirect.route.clone();
                self.fetch(state, &row, &name, &route, &host).await?;
                return Ok(Duration::from_secs(INDEX_MIN_GAP_SECS));
            }
            let route = match self.registry.route(&name) {
                Ok(RouteStep::Known(Some(route))) => route,
                Ok(RouteStep::Known(None)) => {
                    let reason = format!("no RDAP or WHOIS server known for .{}", name.tld());
                    row.record_failure(
                        state,
                        now,
                        now + TimeDelta::days(cadence::NO_ROUTE_RECHECK_DAYS),
                        &reason,
                    )
                    .await?;
                    continue;
                }
                Ok(RouteStep::Ask(query)) => {
                    if let Some(free_at) = self.parked_until(query.host(), now) {
                        host_free_at = Some(host_free_at.map_or(free_at, |at| at.min(free_at)));
                        continue;
                    }
                    self.ask_iana(state, &query).await?;
                    return Ok(Duration::from_secs(INDEX_MIN_GAP_SECS));
                }
                Err(err) if err.is::<BootstrapUnavailable>() => return Err(err),
                Err(err) => {
                    warn!(domain.name = %name, error = %err, "registry route lookup failed");
                    row.record_failure(state, now, row.retry_at(now), &format!("{err:#}"))
                        .await?;
                    continue;
                }
            };
            let host = route.host();
            if let Some(free_at) = self.parked_until(&host, now) {
                host_free_at = Some(host_free_at.map_or(free_at, |at| at.min(free_at)));
                continue;
            }
            self.fetch(state, &row, &name, &route, &host).await?;
            return Ok(Duration::from_secs(INDEX_MIN_GAP_SECS));
        }

        // Every due row waits on a parked host, or none is due: sleep until a host frees up or a row comes due.
        let mut wake_at = now + TimeDelta::seconds(RESYNC_SECS as i64);
        if let Some(next_due) = IndexState::next_due_after(state, now).await? {
            wake_at = wake_at.min(next_due);
        }
        if let Some(free_at) = host_free_at {
            wake_at = wake_at.min(free_at);
        }
        let wait = (wake_at - Utc::now()).to_std().unwrap_or_default();
        Ok(wait.max(Duration::from_secs(1)))
    }

    fn parked_until(&self, host: &str, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        self.hosts
            .get(host)
            .copied()
            .filter(|free_at| *free_at > now)
    }

    /// Stores the host deadline before the request goes out, so a restart during the request still waits for it.
    async fn reserve(
        &mut self,
        state: &AppState,
        host: &str,
        free_at: DateTime<Utc>,
    ) -> Result<(), Error> {
        let parked = IndexHost {
            host: host.to_string(),
            free_at,
        };
        parked.save(state).await?;
        debug!(index.host = host, index.free_at = %free_at, "host reserved for a request");
        self.hosts.insert(parked.host, parked.free_at);
        Ok(())
    }

    /// Sends the one IANA request of a step, after reserving its host. A failure is cached by the registry and only logged here.
    async fn ask_iana(&mut self, state: &AppState, query: &IanaQuery) -> Result<(), Error> {
        let free_at = Utc::now() + TimeDelta::seconds(HOST_MIN_GAP_SECS);
        self.reserve(state, query.host(), free_at).await?;
        if let Err(err) = self.registry.query_iana(query).await {
            warn!(index.host = query.host(), error = %err, "IANA request failed");
        }
        Ok(())
    }

    /// Sends one registry request to `host`, the `Route::host` key of `route`, after reserving it.
    async fn fetch(
        &mut self,
        state: &AppState,
        row: &IndexState,
        name: &DomainName,
        route: &Route,
        host: &str,
    ) -> Result<(), Error> {
        let min_free_at = Utc::now() + TimeDelta::seconds(HOST_MIN_GAP_SECS);
        self.reserve(state, host, min_free_at).await?;
        let result = self.registry.fetch(route, name).await;
        let now = Utc::now();
        if let Some(throttled) = result
            .as_ref()
            .err()
            .and_then(|err| err.downcast_ref::<Throttled>())
        {
            self.reserve(state, host, throttled.host_free_at(now, min_free_at))
                .await?;
        }
        let hops = self
            .redirects
            .remove(&row.name)
            .map_or(0, |redirect| redirect.hops);

        match result {
            Ok(lookup) => row.store(state, lookup, now).await,
            Err(err) => match err.downcast::<Redirected>() {
                // The row stays due, so a later step sends the next hop, paced under the new host.
                Ok(redirect) if hops < MAX_RDAP_REDIRECTS => {
                    info!(domain.name = %name, rdap.to = %redirect.to, "RDAP redirect");
                    let pending = PendingRedirect {
                        route: Route::RdapUrl(redirect.to),
                        hops: hops + 1,
                    };
                    self.redirects.insert(row.name.clone(), pending);
                    Ok(())
                }
                redirect_or_err => {
                    let err = redirect_or_err.map_or_else(
                        |err| err,
                        |redirect| {
                            anyhow!(
                                "more than {MAX_RDAP_REDIRECTS} RDAP redirects, the last to {}",
                                redirect.to
                            )
                        },
                    );
                    warn!(domain.name = %name, error = %err, "registry lookup failed");
                    row.record_failure(state, now, row.retry_at(now), &format!("{err:#}"))
                        .await?;
                    Ok(())
                }
            },
        }
    }
}

impl Default for Indexer {
    fn default() -> Self {
        Self::new()
    }
}

impl IndexState {
    async fn store(
        &self,
        state: &AppState,
        lookup: Lookup,
        now: DateTime<Utc>,
    ) -> Result<(), Error> {
        let mut tx = state.database.pool.begin().await?;
        // Fields are compared only within one source: RDAP and WHOIS report different fields, so a switch starts a new baseline.
        let latest = Snapshot::latest(&mut tx, &self.name, self.kind).await?;
        let comparable = latest
            .as_ref()
            .filter(|latest| latest.source == lookup.route.source());
        let record = match comparable {
            Some(latest) => lookup
                .record
                .keep_omitted(&RegistryRecord::deserialize(&latest.fields.0)?),
            None => lookup.record,
        };
        let observation = Observation {
            name: &self.name,
            kind: self.kind,
            source: lookup.route.source(),
            server: lookup.route.server(),
            fields: serde_json::to_value(&record)?,
            raw: &lookup.raw,
            at: now,
        };
        match comparable {
            Some(latest) if latest.fields.0 == observation.fields => {
                latest.confirm(&mut tx, &observation).await?;
            }
            Some(latest) => {
                let snapshot = Snapshot::insert(&mut tx, &observation).await?;
                let changes = RecordChange::between(&latest.fields, &observation.fields);
                for change in &changes {
                    change.insert(&mut tx, &snapshot).await?;
                }
                info!(
                    domain.name = %self.name,
                    index.kind = %self.kind,
                    index.changes = changes.len(),
                    "registry record changed"
                );
            }
            None => {
                Snapshot::insert(&mut tx, &observation).await?;
                if latest.is_some() {
                    info!(domain.name = %self.name, index.source = %observation.source, "registry source changed, new baseline");
                }
            }
        }
        self.checked(&mut tx, now, record.next_check_at(&self.name, now))
            .await?;
        let status = ManualStatus {
            registered: Some(record.registered),
            lookup_error: None,
        };
        Domain::set_manual_record(
            &mut tx,
            &self.name,
            record.expiry_at,
            record.registered_at,
            &status,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Backs the row off until `next_check_at` and shows the error on a manual row of the same name.
    async fn record_failure(
        &self,
        state: &AppState,
        at: DateTime<Utc>,
        next_check_at: DateTime<Utc>,
        error: &str,
    ) -> Result<(), Error> {
        self.failed(state, at, next_check_at, error).await?;
        Domain::set_manual_error(state, &self.name, error).await?;
        Ok(())
    }
}

impl Throttled {
    /// The later of the host's usual gap and the server's `Retry-After`, or `THROTTLE_PARK_SECS` without one.
    fn host_free_at(&self, now: DateTime<Utc>, min_free_at: DateTime<Utc>) -> DateTime<Utc> {
        let wait = self
            .retry_after
            .and_then(|after| TimeDelta::from_std(after).ok())
            .unwrap_or(TimeDelta::seconds(THROTTLE_PARK_SECS));
        min_free_at.max(now + wait)
    }
}

impl RegistryRecord {
    /// A field the answer leaves out keeps its last known value; only `registered: false` clears fields.
    fn keep_omitted(mut self, previous: &Self) -> Self {
        if self.registered && previous.registered {
            self.expiry_at = self.expiry_at.or(previous.expiry_at);
            self.registered_at = self.registered_at.or(previous.registered_at);
            self.registrar = self.registrar.or_else(|| previous.registrar.clone());
            self.dnssec = self.dnssec.or(previous.dnssec);
            self.statuses = self.statuses.or_else(|| previous.statuses.clone());
            self.nameservers = self.nameservers.or_else(|| previous.nameservers.clone());
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registered(registrar: Option<&str>, dnssec: Option<bool>) -> RegistryRecord {
        RegistryRecord {
            registered: true,
            registrar: registrar.map(str::to_string),
            dnssec,
            ..RegistryRecord::default()
        }
    }

    #[test]
    fn omitted_field_keeps_value_and_next_value_is_one_change() {
        let first = registered(Some("Other Registrar Inc."), Some(false));
        let omitted = registered(None, Some(false)).keep_omitted(&first);
        assert_eq!(omitted, first);

        let next = registered(Some("MarkMonitor Inc."), Some(false)).keep_omitted(&omitted);
        let changes = RecordChange::between(
            &serde_json::to_value(&omitted).unwrap(),
            &serde_json::to_value(&next).unwrap(),
        );
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "registrar");
    }

    #[test]
    fn retry_after_never_shortens_the_host_gap() {
        let now = Utc::now();
        let min_free_at = now + TimeDelta::seconds(HOST_MIN_GAP_SECS);
        let throttled = |retry_after| Throttled {
            url: String::new(),
            retry_after,
        };

        assert_eq!(
            throttled(Some(Duration::ZERO)).host_free_at(now, min_free_at),
            min_free_at
        );
        assert_eq!(
            throttled(Some(Duration::from_secs(7200))).host_free_at(now, min_free_at),
            now + TimeDelta::hours(2)
        );
        assert_eq!(
            throttled(None).host_free_at(now, min_free_at),
            now + TimeDelta::seconds(THROTTLE_PARK_SECS)
        );
    }

    #[test]
    fn omitted_lists_keep_values_and_included_empty_lists_count() {
        let first = RegistryRecord {
            statuses: Some(vec!["active".into()]),
            nameservers: Some(vec!["ns1.example.net".into()]),
            ..registered(None, None)
        };

        assert_eq!(registered(None, None).keep_omitted(&first), first);

        let emptied = RegistryRecord {
            statuses: Some(vec![]),
            ..registered(None, None)
        }
        .keep_omitted(&first);
        assert_eq!(emptied.statuses, Some(vec![]));
        assert_eq!(emptied.nameservers, first.nameservers);
    }

    #[test]
    fn unregistered_answer_clears_fields() {
        let first = registered(Some("MarkMonitor Inc."), Some(true));

        assert_eq!(
            RegistryRecord::unregistered().keep_omitted(&first),
            RegistryRecord::unregistered()
        );
    }
}

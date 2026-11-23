use std::collections::{BTreeMap, BTreeSet};

use anyhow::{anyhow, Context, Error};
use chrono::{DateTime, Duration, Utc};
use serde_json::Value;
use tracing::{info, warn};

#[cfg(feature = "ens-discovery")]
use super::discovery::DiscoveryClient;
use super::{
    rpc::RpcClient,
    schedule::{ENS_DISCOVERY_INTERVAL_HOURS, ENS_ERROR_RETRY_SECS, ENS_UNRESOLVED_RECHECK_DAYS},
    EnsDomain, EnsName, EnsService, EvmAddress,
};
use crate::{
    models::{
        domain::Domain,
        index_state::{IndexKind, IndexState},
    },
    modules::{domains::diff_outcomes, Synced},
    state::AppState,
};

#[derive(Default)]
struct Candidate {
    configured: bool,
    discovered_by: BTreeSet<String>,
    /// Addresses whose discovery answered without this name, so the name stays only for those that still own or manage it on chain.
    if_still_held_by: BTreeSet<String>,
    /// Set for a row stored before `discovered_by` existed that a failed or skipped discovery keeps, so its unknown provenance stays unknown.
    kept_unattributed: bool,
}

/// What one lookup means for the `index_state` row of the name.
enum Recheck {
    At(DateTime<Utc>),
    /// The name leaves tracking, and its row goes with it.
    Untracked,
    Failed(String),
}

/// When discovery runs next.
#[derive(Default)]
pub struct Discovery {
    /// `None` until the first run, so every process asks at once.
    next_at: Option<DateTime<Utc>>,
}

impl Discovery {
    pub fn next_at(&self, now: DateTime<Utc>) -> DateTime<Utc> {
        self.next_at.unwrap_or(now)
    }
}

/// The on-chain lookups of one refresh.
pub struct Lookups {
    pub total: usize,
    pub failed: usize,
}

impl EnsService {
    pub async fn lookup_name(&self, raw_name: &str) -> Result<Option<EnsDomain>, Error> {
        let name = raw_name
            .parse::<EnsName>()
            .map_err(|err| anyhow!("{err}: {raw_name}"))?;
        RpcClient::new(&self.client, self.rpc_url())
            .resolve_name(&name)
            .await
    }

    /// Looks up the names that are due or whose claims changed, or every name with `all_names`.
    pub async fn refresh(
        &self,
        state: &AppState,
        discovery: &mut Discovery,
        all_names: bool,
    ) -> Result<Lookups, Error> {
        let now = Utc::now();
        let previous = Domain::find_by_provider(state, "ens").await?;
        let mut candidates = self.collect_candidates(&previous, discovery, now).await;

        let mut tx = state.database.pool.begin().await?;
        for name in candidates.keys() {
            IndexState::add(&mut tx, name.as_str(), IndexKind::Ens, now).await?;
        }
        let mut rows = BTreeMap::new();
        for row in IndexState::all(&mut *tx, IndexKind::Ens).await? {
            match row.name.parse::<EnsName>() {
                Ok(name) => {
                    rows.insert(name, row);
                }
                // A name that the current `EnsName` rules reject is never looked up, so its row would stay due forever.
                Err(_) => row.delete(&mut tx).await?,
            }
        }
        tx.commit().await?;
        // A name with only an `index_state` row, such as a discovered name whose first lookup failed before a restart, stays while a configured address holds it on chain.
        for name in rows.keys() {
            candidates.entry(name.clone()).or_insert_with(|| Candidate {
                if_still_held_by: self
                    .config
                    .addresses
                    .iter()
                    .map(ToString::to_string)
                    .collect(),
                ..Candidate::default()
            });
        }

        let rpc = RpcClient::new(&self.client, self.rpc_url());
        let mut outcomes = Vec::with_capacity(candidates.len());
        let mut rechecks = Vec::new();
        let mut failed = 0;

        for (name, candidate) in candidates {
            let row = rows
                .remove(&name)
                .expect("the transaction that read the rows added one for every candidate");
            let previous_row = previous.iter().find(|domain| domain.name == name.as_str());
            // A failed lookup waits for its retry, even when its claims change again.
            if !all_names
                && row.next_check_at > now
                && (row.failures > 0 || !candidate.claims_changed(previous_row))
            {
                continue;
            }

            info!(ens.name = %name, "resolving ENS name");
            let ens_domain = match rpc.resolve_name(&name).await {
                Ok(Some(ens_domain)) => ens_domain,
                Ok(None) => {
                    warn!(ens.name = %name, "ENS name is unregistered or past grace");
                    if previous_row.is_some() {
                        outcomes.push(Synced::Gone(name.to_string()));
                    }
                    let claimed = candidate.configured
                        || candidate.kept_unattributed
                        || !candidate.discovered_by.is_empty();
                    let recheck = if claimed {
                        Recheck::At(now + Duration::days(ENS_UNRESOLVED_RECHECK_DAYS))
                    } else {
                        Recheck::Untracked
                    };
                    rechecks.push((row, recheck));
                    continue;
                }
                Err(err) => {
                    failed += 1;
                    match previous_row {
                        Some(_) => {
                            warn!(ens.name = %name, error = %format_args!("{err:#}"), "ENS lookup failed, keeping stored row")
                        }
                        None => {
                            warn!(ens.name = %name, error = %format_args!("{err:#}"), "ENS lookup failed, name stays untracked until a lookup succeeds")
                        }
                    }
                    rechecks.push((row, Recheck::Failed(format!("{err:#}"))));
                    continue;
                }
            };

            let mut discovered_by = candidate.discovered_by;
            discovered_by.extend(
                candidate
                    .if_still_held_by
                    .into_iter()
                    .filter(|address| ens_domain.is_held_by(address)),
            );
            if !candidate.configured && !candidate.kept_unattributed && discovered_by.is_empty() {
                if let Some(stored) = previous_row {
                    // The departing row keeps its stored `discovered_by`, so its change record shows only what changed on chain.
                    outcomes.push(Synced::Departed(
                        ens_domain.into_row(stored.discovered_by())?,
                    ));
                }
                rechecks.push((row, Recheck::Untracked));
                continue;
            }
            rechecks.push((row, Recheck::At(ens_domain.next_check_at(now))));
            outcomes.push(Synced::Listed(ens_domain.into_row(discovered_by)?));
        }

        diff_outcomes(state, "ens", outcomes).await?;

        // Due times are written after the rows, so a crash in between only repeats lookups.
        let retry_at = now + Duration::seconds(ENS_ERROR_RETRY_SECS as i64);
        let mut tx = state.database.pool.begin().await?;
        for (row, recheck) in &rechecks {
            match recheck {
                Recheck::At(at) => row.checked(&mut tx, now, *at).await?,
                Recheck::Untracked => row.delete(&mut tx).await?,
                Recheck::Failed(error) => row.failed(&mut *tx, now, retry_at, error).await?,
            }
        }
        tx.commit().await?;

        let lookups = Lookups {
            total: rechecks.len(),
            failed,
        };
        info!(
            ens.lookup_count = lookups.total,
            ens.failed_count = lookups.failed,
            "completed on-chain ENS refresh"
        );
        Ok(lookups)
    }

    async fn collect_candidates(
        &self,
        previous: &[Domain],
        discovery: &mut Discovery,
        now: DateTime<Utc>,
    ) -> BTreeMap<EnsName, Candidate> {
        let mut candidates: BTreeMap<EnsName, Candidate> = previous
            .iter()
            .filter_map(|domain| domain.name.parse().ok())
            .map(|name| (name, Candidate::default()))
            .collect();
        for name in &self.config.names {
            candidates.entry(name.clone()).or_default().configured = true;
        }

        let discover = discovery.next_at(now) <= now;
        if discover {
            discovery.next_at = Some(now + Duration::hours(ENS_DISCOVERY_INTERVAL_HOURS));
        }
        for address in &self.config.addresses {
            let last = LastListed::new(previous, address, &self.config.names);
            if discover {
                match self.discover_names(address).await {
                    Ok(names) => {
                        let listed: BTreeSet<EnsName> = names.into_iter().collect();
                        info!(
                            ens.address = %address,
                            ens.listed_count = listed.len(),
                            "ENS discovery answered"
                        );
                        let unlisted: Vec<EnsName> = last
                            .by_address
                            .into_iter()
                            .chain(last.unattributed)
                            .filter(|name| !listed.contains(name))
                            .collect();
                        if !unlisted.is_empty() {
                            info!(
                                ens.address = %address,
                                ens.unlisted_count = unlisted.len(),
                                "ENS discovery no longer lists some names, keeping those the address still holds"
                            );
                        }
                        for name in listed {
                            candidates
                                .entry(name)
                                .or_default()
                                .discovered_by
                                .insert(address.to_string());
                        }
                        for name in unlisted {
                            candidates
                                .entry(name)
                                .or_default()
                                .if_still_held_by
                                .insert(address.to_string());
                        }
                        continue;
                    }
                    Err(err) => warn!(
                        ens.address = %address,
                        ens.kept_count = last.by_address.len(),
                        ens.unattributed_count = last.unattributed.len(),
                        error = %format_args!("{err:#}"),
                        "ENS discovery failed, keeping last discovered names"
                    ),
                }
            }
            last.keep(address, &mut candidates);
        }
        candidates
    }

    #[cfg(feature = "ens-discovery")]
    async fn discover_names(&self, address: &EvmAddress) -> Result<Vec<EnsName>, Error> {
        DiscoveryClient::new(&self.client, self.discovery_url())
            .discover_names(address)
            .await
    }

    #[cfg(not(feature = "ens-discovery"))]
    async fn discover_names(&self, _address: &EvmAddress) -> Result<Vec<EnsName>, Error> {
        Err(anyhow!(
            "ens-discovery feature is disabled; rebuild with --features ens-discovery or configure [ens] names"
        ))
    }
}

impl Candidate {
    /// A stored name that gained or may lose a claim in this run is resolved now, so its row only loses a claim on a fresh on-chain answer.
    fn claims_changed(&self, stored: Option<&Domain>) -> bool {
        let Some(row) = stored else {
            return false;
        };
        let unclaimed =
            !self.configured && !self.kept_unattributed && self.discovered_by.is_empty();
        unclaimed
            || !self.if_still_held_by.is_empty()
            || !self.discovered_by.is_subset(&row.discovered_by())
    }
}

struct LastListed {
    by_address: Vec<EnsName>,
    /// Rows stored before `discovered_by` existed name no address, so every configured address may claim them.
    unattributed: Vec<EnsName>,
}

impl LastListed {
    fn new(previous: &[Domain], address: &EvmAddress, configured: &[EnsName]) -> Self {
        let mut by_address = Vec::new();
        let mut unattributed = Vec::new();
        for domain in previous {
            let Ok(name) = domain.name.parse::<EnsName>() else {
                continue;
            };
            let discovered_by = domain.discovered_by();
            if discovered_by.contains(address.as_str()) {
                by_address.push(name);
            } else if discovered_by.is_empty() && !configured.contains(&name) {
                unattributed.push(name);
            }
        }
        Self {
            by_address,
            unattributed,
        }
    }

    /// Keeps the names last listed for `address`, for a run in which its discovery gave no answer.
    fn keep(self, address: &EvmAddress, candidates: &mut BTreeMap<EnsName, Candidate>) {
        for name in self.by_address {
            candidates
                .entry(name)
                .or_default()
                .discovered_by
                .insert(address.to_string());
        }
        for name in self.unattributed {
            candidates.entry(name).or_default().kept_unattributed = true;
        }
    }
}

impl Domain {
    fn discovered_by(&self) -> BTreeSet<String> {
        self.metadata
            .as_ref()
            .and_then(|metadata| metadata.get("discovered_by"))
            .and_then(Value::as_array)
            .map(|addresses| {
                addresses
                    .iter()
                    .filter_map(Value::as_str)
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl EnsDomain {
    fn is_held_by(&self, address: &str) -> bool {
        [&self.metadata.owner, &self.metadata.manager]
            .into_iter()
            .any(|holder| holder.as_deref() == Some(address))
    }

    fn into_row(self, discovered_by: BTreeSet<String>) -> Result<Domain, Error> {
        let external_id = self
            .metadata
            .namehash
            .clone()
            .unwrap_or_else(|| self.name.clone());
        let mut metadata = serde_json::to_value(&self.metadata)
            .context("failed to serialize ENS domain metadata")?;
        if !discovered_by.is_empty() {
            metadata["discovered_by"] = Value::from_iter(discovered_by);
        }
        Ok(Domain::new(
            self.name,
            "ens".to_string(),
            external_id,
            Some(self.expiry_date),
            self.registration_date,
            None,
            None,
            Some(metadata),
        ))
    }
}

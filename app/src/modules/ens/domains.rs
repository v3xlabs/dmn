use std::collections::{BTreeMap, BTreeSet};

use anyhow::{anyhow, Context, Error};
use serde_json::Value;
use tracing::{info, warn};

#[cfg(feature = "ens-discovery")]
use super::discovery::DiscoveryClient;
use super::{rpc::RpcClient, EnsDomain, EnsName, EnsService, EvmAddress};
use crate::{
    models::domain::Domain,
    modules::{DomainService, Synced},
    state::AppState,
};

#[derive(Default)]
struct Candidate {
    configured: bool,
    discovered_by: BTreeSet<String>,
    /// Addresses whose discovery answered without this name, so the name stays only for those that still own or manage it on chain.
    if_still_held_by: BTreeSet<String>,
    /// Set for a row stored before `discovered_by` existed that a failed discovery keeps, so its unknown provenance stays unknown.
    kept_unattributed: bool,
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

    async fn collect_candidates(&self, previous: &[Domain]) -> BTreeMap<EnsName, Candidate> {
        let mut candidates: BTreeMap<EnsName, Candidate> = previous
            .iter()
            .filter_map(|domain| domain.name.parse().ok())
            .map(|name| (name, Candidate::default()))
            .collect();
        for name in &self.config.names {
            candidates.entry(name.clone()).or_default().configured = true;
        }

        #[cfg(feature = "ens-discovery")]
        let discovery = DiscoveryClient::new(&self.client, self.discovery_url());
        for address in &self.config.addresses {
            #[cfg(feature = "ens-discovery")]
            let discovered = discovery.discover_names(address).await;
            #[cfg(not(feature = "ens-discovery"))]
            let discovered: Result<Vec<EnsName>, Error> = Err(anyhow!(
                "ens-discovery feature is disabled; rebuild with --features ens-discovery or configure [ens] names"
            ));

            match discovered {
                Ok(names) => {
                    let listed: BTreeSet<EnsName> = names.into_iter().collect();
                    let last = LastListed::new(previous, address, &self.config.names);
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
                }
                Err(err) => {
                    let last = LastListed::new(previous, address, &self.config.names);
                    warn!(
                        ens.address = %address,
                        ens.kept_count = last.by_address.len(),
                        ens.unattributed_count = last.unattributed.len(),
                        error = %format_args!("{err:#}"),
                        "ENS discovery failed, keeping last discovered names"
                    );
                    for name in last.by_address {
                        candidates
                            .entry(name)
                            .or_default()
                            .discovered_by
                            .insert(address.to_string());
                    }
                    for name in last.unattributed {
                        candidates.entry(name).or_default().kept_unattributed = true;
                    }
                }
            }
        }
        candidates
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

impl DomainService for EnsService {
    async fn ingest_domains(&self, state: &AppState) -> Result<Vec<Synced>, Error> {
        let previous = Domain::find_by_provider(state, "ens").await?;
        let candidates = self.collect_candidates(&previous).await;
        let rpc = RpcClient::new(&self.client, self.rpc_url());
        let total = candidates.len();
        let mut failed = 0;
        let mut synced = Vec::with_capacity(total);

        for (name, candidate) in candidates {
            let previous_row = previous.iter().find(|domain| domain.name == name.as_str());
            let ens_domain = match rpc.resolve_name(&name).await {
                Ok(Some(ens_domain)) => ens_domain,
                Ok(None) => {
                    warn!(ens.name = %name, "ENS name is unregistered or past grace");
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
                    synced.push(Synced::Unchecked(name.to_string()));
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
                if let Some(row) = previous_row {
                    // The departing row keeps its stored `discovered_by`, so its change record shows only what changed on chain.
                    synced.push(Synced::Departed(ens_domain.into_row(row.discovered_by())?));
                }
                continue;
            }
            synced.push(Synced::Listed(ens_domain.into_row(discovered_by)?));
        }

        // A full RPC outage fails the sync, so `dmn ens index` and the schedule both report it as an error.
        if total > 0 && failed == total {
            return Err(anyhow!("every ENS lookup failed"));
        }
        info!(
            ens.domain_count = synced
                .iter()
                .filter(|outcome| matches!(outcome, Synced::Listed(_)))
                .count(),
            ens.failed_count = failed,
            "completed on-chain ENS ingestion"
        );
        Ok(synced)
    }

    async fn ingest_domain_tld_prices_if_enabled(&self, _state: &AppState) -> Result<(), Error> {
        Ok(())
    }
}

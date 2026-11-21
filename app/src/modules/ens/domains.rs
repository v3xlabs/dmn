use std::collections::BTreeSet;

use anyhow::{anyhow, Context, Error};
use tracing::{info, warn};

#[cfg(feature = "ens-discovery")]
use super::discovery::DiscoveryClient;
use super::{rpc::RpcClient, EnsDomain, EnsName, EnsService};
use crate::{models::domain::Domain, modules::DomainService, state::AppState};

impl EnsService {
    pub async fn lookup_name(&self, raw_name: &str) -> Result<Option<EnsDomain>, Error> {
        let name = raw_name
            .parse::<EnsName>()
            .map_err(|err| anyhow!("{err}: {raw_name}"))?;
        RpcClient::new(&self.client, self.rpc_url())
            .resolve_name(&name)
            .await
    }

    pub async fn fetch_all_configured_domains(&self) -> Result<Vec<EnsDomain>, Error> {
        let candidates = self.collect_candidate_names().await?;
        let rpc = RpcClient::new(&self.client, self.rpc_url());
        let mut resolved = Vec::with_capacity(candidates.len());
        for name in candidates {
            match rpc.resolve_name(&name).await? {
                Some(domain) => resolved.push(domain),
                None => {
                    warn!(ens.name = %name, "configured ENS name is unregistered or past grace")
                }
            }
        }
        Ok(resolved)
    }

    async fn collect_candidate_names(&self) -> Result<BTreeSet<EnsName>, Error> {
        #[cfg(feature = "ens-discovery")]
        {
            let mut candidates: BTreeSet<EnsName> = self.config.names.iter().cloned().collect();
            let discovery = DiscoveryClient::new(&self.client, self.discovery_url());
            for address in &self.config.addresses {
                candidates.extend(discovery.discover_names(address).await?);
            }
            Ok(candidates)
        }

        #[cfg(not(feature = "ens-discovery"))]
        {
            if !self.config.addresses.is_empty() {
                warn!(
                    ens.address_count = self.config.addresses.len(),
                    "ENS addresses configured but ens-discovery feature is disabled; rebuild with --features ens-discovery or configure [ens] names"
                );
            }
            Ok(self.config.names.iter().cloned().collect())
        }
    }
}

impl DomainService for EnsService {
    async fn ingest_domains(&self, state: &AppState) -> Result<Vec<Domain>, Error> {
        let ens_domains = self.fetch_all_configured_domains().await?;
        let mut stored = Vec::with_capacity(ens_domains.len());

        for ens_domain in ens_domains {
            let external_id = ens_domain
                .metadata
                .namehash
                .clone()
                .unwrap_or_else(|| ens_domain.name.clone());
            let metadata = serde_json::to_value(&ens_domain.metadata)
                .context("failed to serialize ENS domain metadata")?;

            let domain = Domain::new(
                ens_domain.name,
                "ens".to_string(),
                external_id,
                Some(ens_domain.expiry_date),
                ens_domain.registration_date,
                None,
                None,
                Some(metadata),
                state,
            )
            .await?;
            stored.push(domain);
        }

        info!(
            ens.domain_count = stored.len(),
            "completed on-chain ENS ingestion"
        );
        Ok(stored)
    }

    async fn ingest_domain_tld_prices_if_enabled(&self, _state: &AppState) -> Result<(), Error> {
        Ok(())
    }
}

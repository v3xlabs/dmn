use figment::{Figment, Provider};
use serde::Deserialize;
use tracing::{error, info, warn};

use crate::{
    models::{
        domain::{Domain, ManualStatus, MANUAL_PROVIDER},
        index_state::{IndexKind, IndexState},
        notification::Notification,
        snapshot::Snapshot,
    },
    modules::registry::{name::DomainName, record::RegistryRecord},
    state::AppState,
    Error,
};

/// `domains` is required and other keys are refused, so a missing or misspelled key reads as a config error and not as an empty list.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawManualConfig {
    domains: Vec<String>,
}

/// Domain names tracked through RDAP or WHOIS alone, with no registrar API key.
pub struct ManualService {
    pub names: Vec<DomainName>,
    /// While any entry is no domain name, a sync removes nothing, because a stored row may be that entry.
    unreadable: Vec<String>,
}

impl ManualService {
    pub fn new(entries: Vec<String>) -> Self {
        let mut names = Vec::with_capacity(entries.len());
        let mut unreadable = Vec::new();
        for entry in entries {
            match entry.parse::<DomainName>() {
                Ok(name) if !names.contains(&name) => names.push(name),
                Ok(_) => {}
                Err(reason) => {
                    warn!(domain.name = %entry, reason, "manual domain in config is not a domain name");
                    unreadable.push(entry);
                }
            }
        }
        Self { names, unreadable }
    }

    /// `None` when `[manual]` is missing or does not parse, so stored manual rows stay; only `domains = []` removes them.
    pub fn from_config(provider: &impl Provider) -> Option<Self> {
        let config = Figment::from(provider);
        if !config.contains(MANUAL_PROVIDER) {
            info!("no [manual] section in config, the manual list is off");
            return None;
        }
        match config.focus(MANUAL_PROVIDER).extract::<RawManualConfig>() {
            Ok(raw) => {
                let service = Self::new(raw.domains);
                info!(
                    manual.name_count = service.names.len(),
                    "manual config loaded"
                );
                Some(service)
            }
            Err(err) => {
                error!(error = %err, "manual config does not parse, keeping the stored manual rows");
                None
            }
        }
    }

    pub async fn sync(&self, state: &AppState) -> Result<(), Error> {
        let stored = Domain::find_by_provider(state, MANUAL_PROVIDER).await?;
        let mut tx = state.database.pool.begin().await?;
        // History is shown per name, so a name that a registrar also tracks gets no manual notification.
        let providers = Domain::providers_by_name(&mut tx).await?;
        let tracked_by_registrar = |name: &str| {
            name.parse::<DomainName>()
                .ok()
                .and_then(|name| providers.get(&name))
                .is_some_and(|providers| {
                    providers.iter().any(|provider| provider != MANUAL_PROVIDER)
                })
        };

        let permit = state
            .config
            .deletes
            .as_ref()
            .filter(|_| self.unreadable.is_empty());
        if let Some(permit) = permit {
            for domain in &stored {
                let listed = domain
                    .name
                    .parse::<DomainName>()
                    .is_ok_and(|name| self.names.contains(&name));
                if listed {
                    continue;
                }
                Domain::delete_by_name(&mut tx, permit, MANUAL_PROVIDER, &domain.name).await?;
                info!(domain.name = %domain.name, "manual domain removed");
                if !tracked_by_registrar(&domain.name) {
                    Notification::new(
                        &mut tx,
                        domain.name.clone(),
                        "delete",
                        "Domain deleted".to_string(),
                    )
                    .await?;
                }
            }
        } else if !self.unreadable.is_empty() {
            warn!(
                manual.unreadable = ?self.unreadable,
                "manual list has entries that are no domain name, removing no stored manual rows"
            );
        }

        for name in &self.names {
            let stored_already = stored
                .iter()
                .any(|domain| domain.name.parse::<DomainName>().as_ref() == Ok(name));
            if stored_already {
                continue;
            }
            let record = match Snapshot::latest(&mut tx, name.as_str(), IndexKind::Registry).await?
            {
                Some(snapshot) => Some(RegistryRecord::deserialize(&snapshot.fields.0)?),
                None => None,
            };
            // A name added again keeps the result of its last lookup until the indexer looks again.
            let lookup_error = IndexState::find(state, name.as_str(), IndexKind::Registry)
                .await?
                .and_then(|row| row.last_error);
            let status = ManualStatus {
                registered: record.as_ref().map(|record| record.registered),
                lookup_error,
            };
            Domain::insert_manual(
                &mut tx,
                name.as_str(),
                record.as_ref().and_then(|record| record.expiry_at),
                record.as_ref().and_then(|record| record.registered_at),
                &status,
            )
            .await?;
            info!(domain.name = %name, "manual domain added");
            if !tracked_by_registrar(name.as_str()) {
                Notification::new(
                    &mut tx,
                    name.to_string(),
                    "add",
                    "New domain detected".to_string(),
                )
                .await?;
            }
        }
        tx.commit().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use figment::providers::{Format, Toml};

    use super::ManualService;

    fn names(toml: &str) -> Option<Vec<String>> {
        ManualService::from_config(&Toml::string(toml))
            .map(|service| service.names.iter().map(ToString::to_string).collect())
    }

    #[test]
    fn only_an_explicit_domains_list_is_a_manual_list() {
        assert_eq!(names(""), None);
        assert_eq!(names("[manual]"), None);
        assert_eq!(names("[manual]\ndomain = [\"example.com\"]"), None);
        assert_eq!(names("[manual]\ndomains = \"example.com\""), None);
        assert_eq!(names("[manual]\ndomains = []"), Some(vec![]));
        assert_eq!(
            names("[manual]\ndomains = [\"Example.COM.\", \"example.com\"]"),
            Some(vec!["example.com".to_string()])
        );
    }
}

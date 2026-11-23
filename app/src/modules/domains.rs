use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};
use tracing::info;

use crate::{
    models::{
        domain::{Domain, MANUAL_PROVIDER},
        notification::Notification,
    },
    modules::{registry::name::DomainName, DomainService, Synced},
    state::AppState,
};

const IGNORED_DIFF_KEYS: &[&str] = &["created_at", "updated_at"];

pub struct SyncReport {
    pub notifications: Vec<Notification>,
}

//
pub async fn diff_provider(
    state: &AppState,
    provider: &str,
    provider_domains: &impl DomainService,
) -> Result<SyncReport, anyhow::Error> {
    let outcomes = provider_domains.ingest_domains(state).await?;
    diff_outcomes(state, provider, outcomes).await
}

/// Writes the rows of one sync and their change records in one transaction.
pub async fn diff_outcomes(
    state: &AppState,
    provider: &str,
    outcomes: Vec<Synced>,
) -> Result<SyncReport, anyhow::Error> {
    let pre = Domain::find_by_provider(state, provider).await?;
    let stored = |name: &str| pre.iter().find(|domain| domain.name == name);

    let mut listed = Vec::new();
    let mut departed = Vec::new();
    let mut gone = Vec::new();
    for outcome in outcomes {
        match outcome {
            Synced::Listed(domain) => listed.push(domain),
            Synced::Departed(domain) => departed.push(domain),
            Synced::Gone(name) => gone.push(name),
        }
    }

    let mut additions = Vec::new();
    let mut changes = Vec::new();
    for domain in &listed {
        match stored(&domain.name) {
            Some(before) => changes.push((domain, before.changed_fields(domain))),
            None => additions.push(domain),
        }
    }
    // A name with no stored row has nothing to delete and gets no delete record.
    let mut deletions = Vec::new();
    for domain in &departed {
        if let Some(before) = stored(&domain.name) {
            changes.push((domain, before.changed_fields(domain)));
            deletions.push(domain.name.as_str());
        }
    }
    deletions.extend(gone.iter().map(String::as_str).filter(|name| stored(name).is_some()));
    changes.retain(|(_, change)| !change.is_empty());

    let mut notifications = Vec::new();

    let mut tx = state.database.pool.begin().await?;

    for domain in &listed {
        domain.save(&mut tx).await?;
    }

    for addition in additions {
        info!("New domain detected: {}", addition.name);
        // TODO: notify user the domain was added
        notifications.push(Notification::new(&mut tx, addition.name.clone(), "add", "New domain detected".to_string()).await?);
    }

    for change in changes {
        // println!("Change detected for domain: {} - {:?}", change.0.name, change.1);
        // TODO: notify user the domain was changed

        let human = diff_to_human(change.0.name.clone(), change.1);
        info!("{}", human);

        notifications.push(Notification::new(&mut tx, change.0.name.clone(), "change", human).await?);
    }

    let permit = state.config.deletes.as_ref();
    if permit.is_none() && !deletions.is_empty() {
        info!(
            domain.provider = provider,
            kept = deletions.len(),
            "keeping rows the provider no longer lists, because deletes are off"
        );
    } else if let Some(permit) = permit.filter(|_| !deletions.is_empty()) {
        for deletion in &deletions {
            info!("Domain deleted: {}", deletion);
            Domain::delete_by_name(&mut tx, permit, provider, deletion).await?;
        }

        // A handover is when the manual row is the only row left, the same rule that shows the manual row.
        let remaining = Domain::providers_by_name(&mut tx).await?;
        for deletion in deletions {
            let handover = deletion
                .parse::<DomainName>()
                .ok()
                .and_then(|name| remaining.get(&name))
                .is_some_and(|providers| {
                    providers.iter().all(|provider| provider == MANUAL_PROVIDER)
                });
            let notification = if handover {
                let message =
                    format!("{provider} no longer lists this name, [manual] keeps tracking it");
                Notification::new(&mut tx, deletion.to_string(), "handover", message).await?
            } else {
                Notification::new(&mut tx, deletion.to_string(), "delete", "Domain deleted".to_string()).await?
            };
            notifications.push(notification);
        }
    }

    tx.commit().await?;

    Ok(SyncReport { notifications })
}

impl Domain {
    /// The fields that differ in `after`, as `(before, after)`, leaving out the bookkeeping timestamps.
    fn changed_fields(&self, after: &Self) -> HashMap<String, (Value, Value)> {
        let mut before = self.fields();
        after
            .fields()
            .into_iter()
            .filter(|(key, _)| !IGNORED_DIFF_KEYS.contains(&key.as_str()))
            .filter_map(|(key, after)| {
                let before = before.remove(&key).unwrap_or(Value::Null);
                (before != after).then_some((key, (before, after)))
            })
            .collect()
    }

    fn fields(&self) -> Map<String, Value> {
        match serde_json::to_value(self).expect("a Domain has only string keys") {
            Value::Object(fields) => fields,
            other => unreachable!("domain {} serialized to {other}", self.name),
        }
    }
}

fn diff_to_human(
    domain: String,
    change: HashMap<String, (serde_json::Value, serde_json::Value)>,
) -> String {
    let mut human = String::new();

    for (key, (pre, post)) in change {
        let mut line = format!(" - {}: {:?}", key, (&pre, &post));

        if key == "metadata" {
            // metadata changes (we should calculate the diff between v1 and v2)
            if let (Value::Object(pre), Value::Object(post)) = (&pre, &post) {
                let keys = pre.keys().chain(post.keys()).collect::<HashSet<_>>();
                line = "".to_string();

                for key in keys {
                    let pre_value = pre.get(key).unwrap_or(&Value::Null);
                    let post_value = post.get(key).unwrap_or(&Value::Null);

                    if pre_value != post_value {
                        match key.as_str() {
                            "status" => {
                                if let (Some(a), Some(b)) =
                                    (pre_value.as_str(), post_value.as_str())
                                {
                                    line += &format!(" - Status Changed: {a} => {b}\n");
                                } else {
                                    let (pre_bool, post_bool) =
                                        (bool_from_value(pre_value), bool_from_value(post_value));
                                    line +=
                                        &format!(" - Status Changed: {pre_bool} => {post_bool}\n");
                                }
                            }
                            "security_lock" => {
                                let (pre_bool, post_bool) =
                                    (bool_from_value(pre_value), bool_from_value(post_value));
                                line += &format!(
                                    " - Security Lock Changed: {} => {}\n",
                                    pre_bool, post_bool
                                );
                            }
                            _ => {
                                line += &format!(" - {}: {:?}\n", key, (pre_value, post_value));
                            }
                        }
                    }
                }
            }
        } else if key == "ext_auto_renew" {
            let (pre_bool, post_bool) =
                (bool_from_value(&pre), bool_from_value(&post));
            line = format!(" - Auto Renew Changed: {} => {}\n", pre_bool, post_bool);
        }

        human += &format!("{}\n", line.trim_matches('\n'));
    }

    human.trim_matches('\n').to_string()
}

fn bool_from_value(value: &serde_json::Value) -> bool {
    if value.is_boolean() {
        value.as_bool().unwrap()
    } else if value.is_number() {
        value.as_i64().unwrap() != 0
    } else if value.is_string() {
        value.as_str().unwrap().to_lowercase() == "true"
    } else {
        false
    }
}

use std::collections::{HashMap, HashSet};

use serde_json::Value;
use tracing::info;

use crate::{
    models::{domain::Domain, notification::Notification},
    modules::DomainService,
    state::AppState,
};

const IGNORED_DIFF_KEYS: &[&str] = &["created_at", "updated_at"];

//
pub async fn diff_provider(
    state: &AppState,
    provider: &str,
    provider_domains: &impl DomainService,
) -> Result<Vec<Notification>, anyhow::Error> {
    let pre = Domain::find_by_provider(state, provider).await?;

    let post = provider_domains.ingest_domains(state).await?;

    let mut notifications = Vec::new();

    // diff deletions
    let (additions, deletions, changes) = diff_changes(&pre, &post).await?;

    for deletion in deletions {
        info!("Domain deleted: {}", deletion);
        // TODO: notify user the domain was deleted
        notifications.push(Notification::new(state, deletion, "delete", "Domain deleted".to_string()).await?);
    }

    for addition in additions {
        info!("New domain detected: {}", addition.name);
        // TODO: notify user the domain was added
        notifications.push(Notification::new(state, addition.name, "add", "New domain detected".to_string()).await?);
    }

    for change in changes {
        // println!("Change detected for domain: {} - {:?}", change.0.name, change.1);
        // TODO: notify user the domain was changed

        let human = diff_to_human(change.0.name.clone(), change.1);
        info!("{}", human);

        notifications.push(Notification::new(state, change.0.name, "change", human).await?);
    }

    Ok(notifications)
}

pub async fn diff_changes(
    pre: &[Domain],
    post: &[Domain],
) -> Result<
    (
        Vec<Domain>,
        Vec<String>,
        Vec<(
            Domain,
            HashMap<String, (serde_json::Value, serde_json::Value)>,
        )>,
    ),
    anyhow::Error,
> {
    let mut changes: Vec<(
        Domain,
        HashMap<String, (serde_json::Value, serde_json::Value)>,
    )> = Vec::new();
    let mut additions: Vec<Domain> = Vec::new();
    let mut deletions: Vec<String> = Vec::new();

    for domain in post {
        let pre_domain = pre.iter().find(|d| d.name == domain.name);

        match pre_domain {
            // existing domain
            Some(pre_domain) => {
                // check if there is a difference
                let pre_raw = serde_json::to_value(pre_domain).unwrap();
                let post_raw = serde_json::to_value(domain).unwrap();

                if pre_raw != post_raw {
                    // changes detected
                    // info!("Domain {} has changed", domain.name);

                    let mut change = HashMap::new();

                    // for every key in the post_raw, check if it is a difference
                    for (key, value) in post_raw.as_object().unwrap() {
                        if !IGNORED_DIFF_KEYS.contains(&key.as_str())
                            && pre_raw.get(key).unwrap() != value
                        {
                            change.insert(
                                key.to_string(),
                                (pre_raw.get(key).unwrap().clone(), value.clone()),
                            );
                        }
                    }

                    if !change.is_empty() {
                        changes.push((domain.clone(), change));
                    }
                }
            }
            // new domain
            None => {
                additions.push(domain.clone());
            }
        }
    }

    // find deletions
    for domain in pre {
        if !post.iter().any(|d| d.name == domain.name) {
            deletions.push(domain.name.clone());
        }
    }

    Ok((additions, deletions, changes))
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
                    let pre_value = pre.get(key).unwrap();
                    let post_value = post.get(key).unwrap();

                    if pre_value != post_value {
                        match key.as_str() {
                            "status" => {
                                let (pre_bool, post_bool) =
                                    (bool_from_value(pre_value), bool_from_value(post_value));
                                line +=
                                    &format!(" - Status Changed: {} => {}\n", pre_bool, post_bool);
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

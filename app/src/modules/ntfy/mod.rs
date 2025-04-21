use std::collections::HashMap;

use figment::{providers::Env, Figment};
use ntfy::prelude::*;
use serde::Deserialize;
use tracing::{info, warn};

use crate::models::notification::Notification;

pub struct NtfyService {
    pub dispatcher: Dispatcher<Async>,
    pub topic: String,
}

#[derive(Debug, Deserialize)]
pub struct NtfyConfig {
    pub url: String,
    pub topic: String,
    pub username: String,
    pub password: String,
}

impl NtfyService {
    pub fn init(config: NtfyConfig) -> Self {
        let dispatcher = dispatcher::builder(config.url)
            .credentials(Auth::Credentials {
                username: config.username,
                password: config.password,
            })
            .build_async()
            .unwrap();

        Self {
            dispatcher,
            topic: config.topic,
        }
    }

    pub async fn try_init(provider: &impl figment::Provider) -> Option<Self> {
        let config = Figment::new()
            .merge(Env::prefixed("NTFY_"))
            .merge(provider)
            .extract::<NtfyConfig>();
        if let Ok(config) = config {
            let service = Self::init(config);
            info!("Ntfy config verified");
            Some(service)
        } else {
            warn!("Ntfy config verification failed {:?}", config);
            None
        }
    }

    pub async fn send_notifications(&self, notifications: Vec<Notification>) -> Result<(), Error> {
        let mut notifications_by_topic = HashMap::new();

        for notification in notifications {
            notifications_by_topic
                .entry(notification.event.clone())
                .or_insert(Vec::new())
                .push(notification);
        }

        for (topic, notifications) in notifications_by_topic {
            let plural = if notifications.len() > 1 { "s" } else { "" };
            let topic_name = match topic.as_str() {
                "add" => format!("New Domain{}", plural),
                "delete" => format!("Domain{} Deleted", plural),
                "change" => format!("Domain{} Changed", plural),
                _ => "Unknown".to_string(),
            };

            let message: String = if topic.as_str() == "change" {
                format!(
                    "{}\n\n{}",
                    notifications.iter().map(|n| format!("*{}*", n.domain)).collect::<Vec<String>>().join(", "),
                notifications
                    .iter()
                    .map(|n| format!("**{}**:\n{}", n.domain, n.message))
                    .collect::<Vec<String>>()
                    .join("\n\n")
                )
            } else if ["add", "delete"].contains(&topic.as_str()) {
                // just show list of domains escaped with `
                notifications
                    .iter()
                    .map(|n| format!("- `{}`", n.domain))
                    .collect::<Vec<String>>()
                    .join("\n")
            } else {
                // default fallback notif behaviour
                notifications
                    .iter()
                    .map(|n| format!("`{}`", n.domain))
                    .collect::<Vec<String>>()
                    .join("\n")
            };

            let payload = Payload::new(&self.topic)
                .message(message)
                .title(topic_name)
                .markdown(true);

            self.dispatcher.send(&payload).await?;
        }

        Ok(())
    }
}

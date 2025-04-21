use std::collections::HashMap;

use figment::{providers::Env, Figment};
use serde::Deserialize;
use teloxide::{payloads::SendMessageSetters, prelude::{ChatId, Requester}, types::ParseMode, Bot};
use tracing::{info, warn};

use crate::{models::notification::Notification, state::AppState, Error};

pub struct TelegramService {
    pub bot: Bot,
    pub chat_id: i64,
}

#[derive(Debug, Deserialize)]
pub struct TelegramConfig {
    pub token: String,
    pub chat_id: i64,
}

impl TelegramService {
    pub async fn try_init(provider: &impl figment::Provider) -> Option<Self> {
        let config = Figment::new()
            .merge(Env::prefixed("TELEGRAM_"))
            .merge(provider)
            .extract::<TelegramConfig>();
        if let Ok(config) = config {
            let service = Self::init(&config).await.ok()?;
            info!("Telegram config verified");
            Some(service)
        } else {
            warn!("Telegram config verification failed {:?}", config);
            None
        }
    }

    pub async fn init(config: &TelegramConfig) -> Result<Self, Error> {
        let bot = Bot::new(config.token.clone());
        Ok(Self { bot, chat_id: config.chat_id })
    }

    pub async fn send_notifications(&self, notifications: Vec<Notification>) -> Result<(), Error> {

        let mut notifications_by_topic = HashMap::new();

        for notification in notifications {
            notifications_by_topic.entry(notification.event.clone()).or_insert(Vec::new()).push(notification);
        }

        let mut messages = Vec::new();

        for (topic, notifications) in notifications_by_topic {
            let plural = if notifications.len() > 1 { "s" } else { "" };
            let topic_name = match topic.as_str() {
                "add" => format!("New Domain{}", plural),
                "delete" => format!("Domain{} Deleted", plural),
                "change" => format!("Domain{} Changed", plural),
                _ => "Unknown".to_string(),
            };

            let message: String = if topic.as_str() == "change" {
                notifications.iter().map(|n| format!("`{}`:\n {}", n.domain, escape_markdown(&n.message))).collect::<Vec<String>>().join("\n")
            } else if ["add", "delete"].contains(&topic.as_str()) {
                // just show list of domains escaped with `
                notifications.iter().map(|n| format!("`{}`", n.domain)).collect::<Vec<String>>().join("\n")
            } else {
                // default fallback notif behaviour
                notifications.iter().map(|n| format!("`{}`", n.domain)).collect::<Vec<String>>().join("\n")
            };

            messages.push(format!("**{}**\n{}", topic_name, message));
        }

        let message = messages.join("\n\n");

        // send message to telegram
        self.bot.send_message(self.chat_id.to_string(), message).parse_mode(ParseMode::MarkdownV2).await?;

        Ok(())
    }
}

fn escape_markdown(text: &str) -> String {
    text.replace("*", "\\*")
        .replace("_", "\\_")
        .replace("`", "\\`")
        .replace("~", "\\~")
        .replace(".", "\\.")
        .replace("-", "\\-")
        .replace("=", "\\=")
        .replace(">", "\\>")
        .replace("<", "\\<")
}

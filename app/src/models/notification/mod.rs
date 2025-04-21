use poem_openapi::Object;

use crate::{state::AppState, Error};

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow, Object)]
pub struct Notification {
    pub id: i64,
    pub domain: String,
    pub event: String,
    pub message: String,
    pub created_at: chrono::NaiveDateTime,
}

impl Notification {
    pub async fn new(state: &AppState, domain: String, event: &str, message: String) -> Result<Self, Error> {
        let notification = sqlx::query_as!(
            Notification,
            "INSERT INTO notifications (domain, event, message) VALUES (?, ?, ?) RETURNING *",
            domain,
            event,
            message
        )
        .fetch_one(&state.database.pool)
        .await?;

        Ok(notification)
    }

    pub async fn find_all(state: &AppState) -> Result<Vec<Self>, Error> {
        let notifications = sqlx::query_as!(
            Notification,
            "SELECT * FROM notifications"
        )
        .fetch_all(&state.database.pool)
        .await?;

        Ok(notifications)
    }
}

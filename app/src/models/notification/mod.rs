use poem_openapi::Object;
use sqlx::SqliteConnection;

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
    pub async fn new(conn: &mut SqliteConnection, domain: String, event: &str, message: String) -> Result<Self, Error> {
        let notification = sqlx::query_as!(
            Notification,
            "INSERT INTO notifications (domain, event, message) VALUES (?, ?, ?) RETURNING *",
            domain,
            event,
            message
        )
        .fetch_one(conn)
        .await?;

        Ok(notification)
    }

    pub async fn latest_per_domain(state: &AppState, limit: u32) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as::<_, Notification>(
            "SELECT id, domain, event, message, created_at FROM (
                SELECT *, ROW_NUMBER() OVER (PARTITION BY domain ORDER BY created_at DESC, id DESC) AS position
                FROM notifications
            ) WHERE position <= ?",
        )
        .bind(limit)
        .fetch_all(&state.database.pool)
        .await
    }
}

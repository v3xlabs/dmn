use chrono::{DateTime, Utc};
use poem_openapi::Object;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::prelude::FromRow;

use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize, FromRow, Object)]
pub struct Domain {
    pub name: String,
    pub provider: String,
    pub external_id: Option<String>,
    pub ext_expiry_at: Option<DateTime<Utc>>,
    pub ext_registered_at: Option<DateTime<Utc>>,
    pub ext_auto_renew: Option<bool>,
    pub metadata: Option<Value>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl Domain {
    pub async fn new(
        name: String,
        provider: String,
        external_id: String,
        ext_expiry_at: Option<DateTime<Utc>>,
        ext_registered_at: Option<DateTime<Utc>>,
        ext_auto_renew: Option<bool>,
        metadata: Option<Value>,
        state: &AppState,
    ) -> Result<Self, sqlx::Error> {
        let domain = sqlx::query_as::<_, Domain>(
            "INSERT OR REPLACE INTO domains (name, provider, external_id, ext_expiry_at, ext_registered_at, ext_auto_renew, metadata) VALUES (?, ?, ?, ?, ?, ?, ?) RETURNING name, provider, external_id, ext_expiry_at, ext_registered_at, ext_auto_renew, metadata, created_at, updated_at"
        )
        .bind(&name)
        .bind(&provider)
        .bind(&external_id)
        .bind(&ext_expiry_at)
        .bind(&ext_registered_at)
        .bind(&ext_auto_renew)
        .bind(&metadata)
        .fetch_one(&state.database.pool)
        .await?;

        Ok(domain)
    }

    pub async fn get_all(state: &AppState) -> Result<Vec<Self>, sqlx::Error> {
        let domains = sqlx::query_as::<_, Domain>(
            "SELECT name, provider, external_id, ext_expiry_at, ext_registered_at, ext_auto_renew, metadata, created_at, updated_at FROM domains ORDER BY ext_expiry_at ASC"
        )
        .fetch_all(&state.database.pool)
        .await?;

        Ok(domains)
    }
}

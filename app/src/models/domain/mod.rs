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
    pub metadata: Option<Value>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl Domain {
    pub async fn new(
        name: String,
        provider: String,
        external_id: String,
        metadata: Option<Value>,
        state: &AppState,
    ) -> Result<Self, sqlx::Error> {
        let domain = sqlx::query_as!(
            Domain,
            "INSERT INTO domains (name, provider, external_id, metadata) VALUES ($1, $2, $3, $4) ON CONFLICT (name, provider) DO UPDATE SET metadata = $4 RETURNING name, provider, external_id, metadata, created_at, updated_at",
            name,
            provider,
            external_id,
            metadata,
        )
        .fetch_one(&state.database.pool)
        .await?;

        Ok(domain)
    }
}

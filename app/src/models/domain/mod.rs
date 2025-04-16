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
        metadata: Option<Value>,
        state: &AppState,
    ) -> Result<Self, sqlx::Error> {
        let domain = sqlx::query_as!(
            Domain,
            "INSERT INTO domains (name, provider, external_id, ext_expiry_at, ext_registered_at, metadata) VALUES ($1, $2, $3, $4, $5, $6) ON CONFLICT (name, provider) DO UPDATE SET ext_expiry_at = $4, ext_registered_at = $5, metadata = $6 RETURNING name, provider, external_id, ext_expiry_at, ext_registered_at, metadata, created_at, updated_at",
            name,
            provider,
            external_id,
            ext_expiry_at,
            ext_registered_at,
            metadata,
        )
        .fetch_one(&state.database.pool)
        .await?;

        Ok(domain)
    }
}

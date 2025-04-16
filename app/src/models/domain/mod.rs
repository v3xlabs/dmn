use chrono::{DateTime, Utc};
use poem_openapi::Object;
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;

#[derive(Debug, Serialize, Deserialize, FromRow, Object)]
pub struct Domain {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub external_id: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

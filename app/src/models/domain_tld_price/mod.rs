use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;

use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct DomainTLDPrice {
    pub provider: String,
    pub tld: String,
    pub price: i64,
    pub updated_at: Option<DateTime<Utc>>,
    pub created_at: Option<DateTime<Utc>>,
}

impl DomainTLDPrice {
    pub async fn new(
        provider: String,
        tld: String,
        price: i64,
        state: &AppState,
    ) -> Result<Self, sqlx::Error> {
        let domain_tld_price = sqlx::query_as(
            "INSERT OR REPLACE INTO domain_tld_prices (provider, tld, price) VALUES (?, ?, ?) RETURNING *"
        )
        .bind(provider)
        .bind(tld)
        .bind(price)
        .fetch_one(&state.database.pool)
        .await?;

        Ok(domain_tld_price)
    }
}

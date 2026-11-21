use chrono::{DateTime, SubsecRound, Utc};
use sqlx::prelude::FromRow;

use crate::state::AppState;

/// When the indexer may query an RDAP or WHOIS host again. Stored so a restart keeps a long `Retry-After`.
#[derive(Debug, Clone, FromRow)]
pub struct IndexHost {
    pub host: String,
    pub free_at: DateTime<Utc>,
}

impl IndexHost {
    pub async fn all(state: &AppState) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as::<_, Self>("SELECT host, free_at FROM index_hosts")
            .fetch_all(&state.database.pool)
            .await
    }

    pub async fn save(&self, state: &AppState) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO index_hosts (host, free_at) VALUES (?, ?)
             ON CONFLICT (host) DO UPDATE SET free_at = excluded.free_at",
        )
        .bind(&self.host)
        .bind(self.free_at.trunc_subsecs(0))
        .execute(&state.database.pool)
        .await?;
        Ok(())
    }
}

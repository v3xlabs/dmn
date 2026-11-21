use serde_json::Value;
use sqlx::prelude::FromRow;

use crate::{modules::cloudflare::dns::ZoneListing, state::AppState};

#[derive(Debug, FromRow)]
pub struct DnsDomain {
    pub name: String,
    pub provider: String,
    pub external_id: Option<String>,
    pub metadata: Option<Value>,
}

impl DnsDomain {
    /// Replaces every Cloudflare row in one transaction, so zones removed upstream disappear and readers never see a partial list.
    pub async fn replace_cloudflare_zones(
        state: &AppState,
        listing: ZoneListing,
    ) -> Result<usize, sqlx::Error> {
        let domains: Vec<Self> = listing.into_zones().into_iter().map(Self::from).collect();
        let mut transaction = state.database.pool.begin().await?;
        sqlx::query("DELETE FROM dns_domains WHERE provider = 'cloudflare'")
            .execute(&mut *transaction)
            .await?;
        for domain in &domains {
            sqlx::query(
                "INSERT INTO dns_domains (name, provider, external_id, metadata) VALUES (?, ?, ?, ?)",
            )
            .bind(&domain.name)
            .bind(&domain.provider)
            .bind(&domain.external_id)
            .bind(&domain.metadata)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(domains.len())
    }

    pub async fn find_by_provider(
        state: &AppState,
        provider: &str,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as::<_, Self>("SELECT * FROM dns_domains WHERE provider = ?")
            .bind(provider)
            .fetch_all(&state.database.pool)
            .await
    }
}

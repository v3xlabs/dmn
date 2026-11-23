use chrono::{DateTime, Utc};
use poem_openapi::Object;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{prelude::FromRow, SqliteConnection};

use std::collections::{HashMap, HashSet};

use crate::{
    modules::registry::name::DomainName,
    state::{AppState, DeletesAllowed},
};

pub const MANUAL_PROVIDER: &str = "manual";

/// `metadata` of a manual row: what the latest registry lookup said about the name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManualStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub registered: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lookup_error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, FromRow, Object, Clone)]
pub struct Domain {
    pub name: String,
    pub provider: String,
    pub external_id: Option<String>,
    pub ext_expiry_at: Option<DateTime<Utc>>,
    pub ext_registered_at: Option<DateTime<Utc>>,
    pub ext_auto_renew: Option<bool>,
    pub ext_whois_privacy: Option<bool>,
    pub metadata: Option<Value>,
    pub created_at: Option<DateTime<Utc>>,
    pub updated_at: Option<DateTime<Utc>>,
}

impl Domain {
    pub fn new(
        name: String,
        provider: String,
        external_id: String,
        ext_expiry_at: Option<DateTime<Utc>>,
        ext_registered_at: Option<DateTime<Utc>>,
        ext_auto_renew: Option<bool>,
        ext_whois_privacy: Option<bool>,
        metadata: Option<Value>,
    ) -> Self {
        Self {
            name,
            provider,
            external_id: Some(external_id),
            ext_expiry_at,
            ext_registered_at,
            ext_auto_renew,
            ext_whois_privacy,
            metadata,
            created_at: None,
            updated_at: None,
        }
    }

    pub async fn save(&self, conn: &mut SqliteConnection) -> Result<Self, sqlx::Error> {
        sqlx::query_as::<_, Domain>(
            "INSERT OR REPLACE INTO domains (name, provider, external_id, ext_expiry_at, ext_registered_at, ext_auto_renew, ext_whois_privacy, metadata) VALUES (?, ?, ?, ?, ?, ?, ?, ?) RETURNING *"
        )
        .bind(&self.name)
        .bind(&self.provider)
        .bind(&self.external_id)
        .bind(self.ext_expiry_at)
        .bind(self.ext_registered_at)
        .bind(self.ext_auto_renew)
        .bind(self.ext_whois_privacy)
        .bind(&self.metadata)
        .fetch_one(conn)
        .await
    }

    /// A manual row is left out when a registrar provider has the same `DomainName`.
    pub async fn get_all(state: &AppState) -> Result<Vec<Self>, sqlx::Error> {
        let domains =
            sqlx::query_as::<_, Domain>("SELECT * FROM domains ORDER BY ext_expiry_at ASC")
                .fetch_all(&state.database.pool)
                .await?;
        let registrar_names: HashSet<DomainName> = domains
            .iter()
            .filter(|domain| domain.provider != MANUAL_PROVIDER)
            .filter_map(|domain| domain.name.parse().ok())
            .collect();

        Ok(domains
            .into_iter()
            .filter(|domain| {
                domain.provider != MANUAL_PROVIDER
                    || domain
                        .name
                        .parse::<DomainName>()
                        .map_or(true, |name| !registrar_names.contains(&name))
            })
            .collect())
    }

    /// Leaves an existing row alone, so a restart keeps the dates and status the indexer wrote.
    pub async fn insert_manual(
        conn: &mut SqliteConnection,
        name: &str,
        expiry_at: Option<DateTime<Utc>>,
        registered_at: Option<DateTime<Utc>>,
        status: &ManualStatus,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO domains (name, provider, external_id, ext_expiry_at, ext_registered_at, metadata)
             VALUES (?1, ?2, ?1, ?3, ?4, ?5) ON CONFLICT (provider, name) DO NOTHING"
        )
        .bind(name)
        .bind(MANUAL_PROVIDER)
        .bind(expiry_at)
        .bind(registered_at)
        .bind(sqlx::types::Json(status))
        .execute(conn)
        .await?;

        Ok(())
    }

    pub async fn set_manual_record(
        conn: &mut SqliteConnection,
        name: &str,
        expiry_at: Option<DateTime<Utc>>,
        registered_at: Option<DateTime<Utc>>,
        status: &ManualStatus,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE domains SET ext_expiry_at = ?, ext_registered_at = ?, metadata = ?, updated_at = CURRENT_TIMESTAMP
             WHERE provider = ? AND name = ?"
        )
        .bind(expiry_at)
        .bind(registered_at)
        .bind(sqlx::types::Json(status))
        .bind(MANUAL_PROVIDER)
        .bind(name)
        .execute(conn)
        .await?;

        Ok(())
    }

    pub async fn set_manual_error(
        state: &AppState,
        name: &str,
        error: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE domains SET metadata = json_set(COALESCE(metadata, '{}'), '$.lookup_error', ?)
             WHERE provider = ? AND name = ?",
        )
        .bind(error)
        .bind(MANUAL_PROVIDER)
        .bind(name)
        .execute(&state.database.pool)
        .await?;

        Ok(())
    }

    pub async fn providers_by_name(
        conn: &mut SqliteConnection,
    ) -> Result<HashMap<DomainName, Vec<String>>, sqlx::Error> {
        let rows: Vec<(String, String)> = sqlx::query_as("SELECT name, provider FROM domains")
            .fetch_all(conn)
            .await?;
        let mut providers: HashMap<DomainName, Vec<String>> = HashMap::new();
        for (name, provider) in rows {
            if let Ok(name) = name.parse() {
                providers.entry(name).or_default().push(provider);
            }
        }
        Ok(providers)
    }

    pub async fn find_by_provider(state: &AppState, provider: &str) -> Result<Vec<Self>, sqlx::Error> {
        let domains = sqlx::query_as::<_, Domain>(
            "SELECT * FROM domains WHERE provider = ?"
        )
        .bind(provider)
        .fetch_all(&state.database.pool)
        .await?;

        Ok(domains)
    }

    pub async fn delete_by_name(
        conn: &mut SqliteConnection,
        _: &DeletesAllowed,
        provider: &str,
        name: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM domains WHERE provider = ? AND name = ?")
            .bind(provider)
            .bind(name)
            .execute(conn)
            .await?;

        Ok(())
    }
}

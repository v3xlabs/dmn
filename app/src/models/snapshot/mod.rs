use chrono::{DateTime, SubsecRound, Utc};
use serde_json::Value;
use sqlx::{prelude::FromRow, types::Json, SqliteConnection};

use crate::{models::index_state::IndexKind, modules::registry::Source};

/// One distinct set of normalized fields for a name, with the raw answer of its latest fetch.
#[derive(Debug, Clone, FromRow)]
pub struct Snapshot {
    pub id: i64,
    pub name: String,
    pub kind: IndexKind,
    pub source: Source,
    pub server: String,
    pub fields: Json<Value>,
    pub raw: String,
    pub observed_at: DateTime<Utc>,
    pub confirmed_at: DateTime<Utc>,
}

/// A fetched answer about to be stored.
pub struct Observation<'a> {
    pub name: &'a str,
    pub kind: IndexKind,
    pub source: Source,
    pub server: &'a str,
    pub fields: Value,
    pub raw: &'a str,
    pub at: DateTime<Utc>,
}

impl Snapshot {
    pub async fn latest(
        conn: &mut SqliteConnection,
        name: &str,
        kind: IndexKind,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            "SELECT * FROM snapshots WHERE name = ? AND kind = ? ORDER BY id DESC LIMIT 1",
        )
        .bind(name)
        .bind(kind)
        .fetch_optional(conn)
        .await
    }

    pub async fn insert(
        conn: &mut SqliteConnection,
        observation: &Observation<'_>,
    ) -> Result<Self, sqlx::Error> {
        let at = observation.at.trunc_subsecs(0);
        sqlx::query_as::<_, Self>(
            "INSERT INTO snapshots (name, kind, source, server, fields, raw, observed_at, confirmed_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?) RETURNING *",
        )
        .bind(observation.name)
        .bind(observation.kind)
        .bind(observation.source)
        .bind(observation.server)
        .bind(Json(&observation.fields))
        .bind(observation.raw)
        .bind(at)
        .bind(at)
        .fetch_one(conn)
        .await
    }

    /// Records a fetch whose fields equal this snapshot. The raw answer is replaced, because RDAP embeds the query time in it.
    pub async fn confirm(
        &self,
        conn: &mut SqliteConnection,
        observation: &Observation<'_>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE snapshots SET confirmed_at = ?, raw = ?, source = ?, server = ? WHERE id = ?",
        )
        .bind(observation.at.trunc_subsecs(0))
        .bind(observation.raw)
        .bind(observation.source)
        .bind(observation.server)
        .bind(self.id)
        .execute(conn)
        .await?;
        Ok(())
    }
}

use chrono::{DateTime, SubsecRound, Utc};
use serde_json::Value;
use sqlx::{prelude::FromRow, types::Json, SqliteConnection};

use crate::{
    models::{domain::Domain, index_state::IndexKind},
    modules::registry::{name::DomainName, Source},
    state::AppState,
};

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

/// The latest snapshot of a name without its raw answer, which the page loads only on request.
#[derive(Debug, Clone, FromRow)]
pub struct SnapshotSummary {
    pub id: i64,
    pub name: String,
    pub source: Source,
    pub server: String,
    pub fields: Json<Value>,
    pub confirmed_at: DateTime<Utc>,
}

impl SnapshotSummary {
    pub async fn latest_per_name(
        state: &AppState,
        kind: IndexKind,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            "SELECT id, name, source, server, fields, confirmed_at FROM snapshots
             WHERE id IN (SELECT MAX(id) FROM snapshots WHERE kind = ? GROUP BY name)",
        )
        .bind(kind)
        .fetch_all(&state.database.pool)
        .await
    }
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
    /// Snapshot `id` of `name` while a provider tracks the name, so retained answers of a dropped name are not served.
    pub async fn find_tracked(
        conn: &mut SqliteConnection,
        id: i64,
        name: &DomainName,
        kind: IndexKind,
    ) -> Result<Option<Self>, sqlx::Error> {
        if !Domain::is_tracked(&mut *conn, name).await? {
            return Ok(None);
        }
        sqlx::query_as::<_, Self>("SELECT * FROM snapshots WHERE id = ? AND name = ? AND kind = ?")
            .bind(id)
            .bind(name.as_str())
            .bind(kind)
            .fetch_optional(conn)
            .await
    }

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

#[cfg(test)]
mod tests {
    use sqlx::sqlite::SqlitePoolOptions;

    use super::*;

    #[async_std::test]
    async fn raw_answers_are_served_only_for_tracked_names() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::raw_sql(
            "INSERT INTO domains (name, provider) VALUES ('EXAMPLE.COM', 'porkbun'), ('bücher.de', 'porkbun');
             INSERT INTO snapshots (id, name, kind, source, server, fields, raw, observed_at, confirmed_at) VALUES
                (1, 'example.com', 'registry', 'rdap', 's', '{}', 'tracked', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                (2, 'gone.com', 'registry', 'rdap', 's', '{}', 'dropped', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'),
                (3, 'xn--bcher-kva.de', 'registry', 'rdap', 's', '{}', 'idn', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');",
        )
        .execute(&pool)
        .await
        .unwrap();
        let mut conn = pool.acquire().await.unwrap();
        async fn raw(conn: &mut SqliteConnection, id: i64, name: &str) -> Option<String> {
            let name: DomainName = name.parse().unwrap();
            Snapshot::find_tracked(conn, id, &name, IndexKind::Registry)
                .await
                .unwrap()
                .map(|snapshot| snapshot.raw)
        }

        assert_eq!(
            raw(&mut conn, 1, "example.com").await.as_deref(),
            Some("tracked")
        );
        assert_eq!(raw(&mut conn, 3, "bücher.de").await.as_deref(), Some("idn"));
        assert_eq!(raw(&mut conn, 2, "gone.com").await, None);
        assert_eq!(raw(&mut conn, 2, "example.com").await, None);
        assert_eq!(raw(&mut conn, 3, "example.com").await, None);
    }
}

use chrono::{DateTime, SubsecRound, Utc};
use sqlx::{prelude::FromRow, SqliteConnection, SqliteExecutor};
use strum::Display;

use std::collections::BTreeSet;

use crate::{modules::registry::name::DomainName, state::AppState};

/// What an index row tracks for a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Display, sqlx::Type)]
#[strum(serialize_all = "lowercase")]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
pub enum IndexKind {
    Registry,
    Ens,
}

/// ENS names live on Ethereum, not in the DNS root, so registries and resolvers know nothing about them.
const NOT_INDEXED_PROVIDER: &str = "ens";

#[derive(Debug, Clone, FromRow)]
pub struct IndexState {
    pub name: String,
    pub kind: IndexKind,
    pub next_check_at: DateTime<Utc>,
    pub last_checked_at: Option<DateTime<Utc>>,
    pub failures: i64,
    pub last_error: Option<String>,
}

impl IndexState {
    /// Adds every tracked name as due now and drops rows whose name no provider tracks any more.
    pub async fn sync(
        state: &AppState,
        kind: IndexKind,
        now: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        let mut tx = state.database.pool.begin().await?;
        let tracked: Vec<String> =
            sqlx::query_scalar("SELECT DISTINCT name FROM domains WHERE provider != ?")
                .bind(NOT_INDEXED_PROVIDER)
                .fetch_all(&mut *tx)
                .await?;
        let tracked: BTreeSet<DomainName> = tracked
            .iter()
            .filter_map(|name| name.parse().ok())
            .collect();
        for name in &tracked {
            sqlx::query(
                "INSERT OR IGNORE INTO index_state (name, kind, next_check_at) VALUES (?, ?, ?)",
            )
            .bind(name.as_str())
            .bind(kind)
            .bind(now.trunc_subsecs(0))
            .execute(&mut *tx)
            .await?;
        }
        let indexed: Vec<String> =
            sqlx::query_scalar("SELECT name FROM index_state WHERE kind = ?")
                .bind(kind)
                .fetch_all(&mut *tx)
                .await?;
        for name in indexed {
            // A key in another spelling than its `DomainName` is not canonical and goes too.
            let canonical = name.parse::<DomainName>().ok();
            let keep = canonical.is_some_and(|key| key.as_str() == name && tracked.contains(&key));
            if !keep {
                sqlx::query("DELETE FROM index_state WHERE name = ? AND kind = ?")
                    .bind(&name)
                    .bind(kind)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        tx.commit().await
    }

    pub async fn find(
        state: &AppState,
        name: &str,
        kind: IndexKind,
    ) -> Result<Option<Self>, sqlx::Error> {
        sqlx::query_as::<_, Self>("SELECT * FROM index_state WHERE name = ? AND kind = ?")
            .bind(name)
            .bind(kind)
            .fetch_optional(&state.database.pool)
            .await
    }

    /// Adds `name` as due at `now`, unless it has a row already.
    pub async fn add(
        conn: &mut SqliteConnection,
        name: &str,
        kind: IndexKind,
        now: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT OR IGNORE INTO index_state (name, kind, next_check_at) VALUES (?, ?, ?)",
        )
        .bind(name)
        .bind(kind)
        .bind(now.trunc_subsecs(0))
        .execute(conn)
        .await?;
        Ok(())
    }

    pub async fn all(
        executor: impl SqliteExecutor<'_>,
        kind: IndexKind,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as::<_, Self>("SELECT * FROM index_state WHERE kind = ?")
            .bind(kind)
            .fetch_all(executor)
            .await
    }

    /// Every due row, so rows behind a parked host never hide due rows on other hosts.
    pub async fn due(
        state: &AppState,
        kind: IndexKind,
        now: DateTime<Utc>,
    ) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            "SELECT * FROM index_state WHERE kind = ? AND next_check_at <= ? ORDER BY next_check_at",
        )
        .bind(kind)
        .bind(now.trunc_subsecs(0))
        .fetch_all(&state.database.pool)
        .await
    }

    pub async fn next_due_after(
        state: &AppState,
        kind: IndexKind,
        now: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
        sqlx::query_scalar(
            "SELECT MIN(next_check_at) FROM index_state WHERE kind = ? AND next_check_at > ?",
        )
        .bind(kind)
        .bind(now.trunc_subsecs(0))
        .fetch_one(&state.database.pool)
        .await
    }

    pub async fn earliest_check_at(
        state: &AppState,
        kind: IndexKind,
    ) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
        sqlx::query_scalar("SELECT MIN(next_check_at) FROM index_state WHERE kind = ?")
            .bind(kind)
            .fetch_one(&state.database.pool)
            .await
    }

    pub async fn delete(&self, conn: &mut SqliteConnection) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM index_state WHERE name = ? AND kind = ?")
            .bind(&self.name)
            .bind(self.kind)
            .execute(conn)
            .await?;
        Ok(())
    }

    pub async fn checked(
        &self,
        conn: &mut SqliteConnection,
        at: DateTime<Utc>,
        next_check_at: DateTime<Utc>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE index_state SET last_checked_at = ?, next_check_at = ?, failures = 0, last_error = NULL
             WHERE name = ? AND kind = ?",
        )
        .bind(at.trunc_subsecs(0))
        .bind(next_check_at.trunc_subsecs(0))
        .bind(&self.name)
        .bind(self.kind)
        .execute(conn)
        .await?;
        Ok(())
    }

    pub async fn failed(
        &self,
        executor: impl SqliteExecutor<'_>,
        at: DateTime<Utc>,
        next_check_at: DateTime<Utc>,
        error: &str,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "UPDATE index_state SET last_checked_at = ?, next_check_at = ?, failures = failures + 1, last_error = ?
             WHERE name = ? AND kind = ?",
        )
        .bind(at.trunc_subsecs(0))
        .bind(next_check_at.trunc_subsecs(0))
        .bind(error)
        .bind(&self.name)
        .bind(self.kind)
        .execute(executor)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{path::PathBuf, sync::Arc};

    use chrono::TimeZone;
    use figment::Figment;
    use sqlx::sqlite::SqlitePoolOptions;

    use super::*;
    use crate::{
        cache::AppCache,
        database::Database,
        state::{AppStateInner, LoadedConfig},
    };

    #[async_std::test]
    async fn two_provider_spellings_share_one_index_row() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::raw_sql(
            "INSERT INTO domains (name, provider) VALUES ('EXAMPLE.TEST', 'porkbun'), ('example.test', 'cloudflare');
             INSERT INTO index_state (name, kind, next_check_at) VALUES ('EXAMPLE.TEST', 'registry', '2030-01-01T00:00:00Z');",
        )
        .execute(&pool)
        .await
        .unwrap();
        let state = Arc::new(AppStateInner {
            database: Database { pool },
            api: None,
            cache: AppCache::new(),
            porkbun: None,
            cloudflare: None,
            ens: None,
            manual: None,
            ntfy: None,
            config: LoadedConfig {
                figment: Figment::new(),
                path: PathBuf::new(),
                deletes: None,
            },
        });
        let now = Utc.with_ymd_and_hms(2026, 11, 22, 0, 0, 0).unwrap();

        IndexState::sync(&state, IndexKind::Registry, now)
            .await
            .unwrap();

        let rows: Vec<(String, DateTime<Utc>)> =
            sqlx::query_as("SELECT name, next_check_at FROM index_state")
                .fetch_all(&state.database.pool)
                .await
                .unwrap();
        assert_eq!(rows, [("example.test".to_string(), now)]);
    }
}

use chrono::{DateTime, SubsecRound, Utc};
use sqlx::{prelude::FromRow, SqliteConnection};
use strum::Display;

use crate::state::AppState;

/// What an index row tracks for a name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Display, sqlx::Type)]
#[strum(serialize_all = "lowercase")]
#[sqlx(type_name = "TEXT", rename_all = "lowercase")]
pub enum IndexKind {
    Registry,
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
        sqlx::query(
            "INSERT OR IGNORE INTO index_state (name, kind, next_check_at)
             SELECT DISTINCT name, ?, ? FROM domains WHERE provider != ?",
        )
        .bind(kind)
        .bind(now.trunc_subsecs(0))
        .bind(NOT_INDEXED_PROVIDER)
        .execute(&state.database.pool)
        .await?;
        sqlx::query(
            "DELETE FROM index_state WHERE kind = ?
             AND name NOT IN (SELECT name FROM domains WHERE provider != ?)",
        )
        .bind(kind)
        .bind(NOT_INDEXED_PROVIDER)
        .execute(&state.database.pool)
        .await?;
        Ok(())
    }

    /// Every due row, so rows behind a parked host never hide due rows on other hosts.
    pub async fn due(state: &AppState, now: DateTime<Utc>) -> Result<Vec<Self>, sqlx::Error> {
        sqlx::query_as::<_, Self>(
            "SELECT * FROM index_state WHERE next_check_at <= ? ORDER BY next_check_at",
        )
        .bind(now.trunc_subsecs(0))
        .fetch_all(&state.database.pool)
        .await
    }

    pub async fn next_due_after(
        state: &AppState,
        now: DateTime<Utc>,
    ) -> Result<Option<DateTime<Utc>>, sqlx::Error> {
        sqlx::query_scalar("SELECT MIN(next_check_at) FROM index_state WHERE next_check_at > ?")
            .bind(now.trunc_subsecs(0))
            .fetch_one(&state.database.pool)
            .await
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
        state: &AppState,
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
        .execute(&state.database.pool)
        .await?;
        Ok(())
    }
}

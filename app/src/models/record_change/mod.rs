use chrono::{DateTime, SubsecRound, Utc};
use serde_json::Value;
use sqlx::{prelude::FromRow, types::Json, SqliteConnection};

use crate::models::{index_state::IndexKind, snapshot::Snapshot};

/// One changed field between two snapshots of a name. Rows are append-only, so a reader keeps a cursor on `id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordChange {
    pub field: String,
    pub before: Value,
    pub after: Value,
}

#[derive(Debug, Clone, FromRow)]
pub struct LoggedChange {
    pub name: String,
    pub field: String,
    pub before: Json<Value>,
    pub after: Json<Value>,
    pub snapshot_id: i64,
    pub observed_at: DateTime<Utc>,
}

impl LoggedChange {
    /// The changes of the newest `limit` snapshots of every name, newest first, so no history entry is cut.
    pub async fn latest_per_name(
        conn: &mut SqliteConnection,
        kind: IndexKind,
        limit: u32,
    ) -> Result<Vec<Self>, sqlx::Error> {
        // `JSON` columns have NUMERIC affinity, so SQLite stores the JSON text `1` as an integer; the cast reads it back as JSON text.
        sqlx::query_as::<_, Self>(
            "SELECT name, field, CAST(before AS TEXT) AS before, CAST(after AS TEXT) AS after, snapshot_id, observed_at
             FROM (
                SELECT *, DENSE_RANK() OVER (PARTITION BY name ORDER BY snapshot_id DESC) AS position
                FROM record_changes WHERE kind = ?
            ) WHERE position <= ? ORDER BY snapshot_id DESC, id",
        )
        .bind(kind)
        .bind(limit)
        .fetch_all(conn)
        .await
    }
}

impl RecordChange {
    /// Every field whose value differs, `null` included, so a value that disappears and returns is two changes.
    pub fn between(before: &Value, after: &Value) -> Vec<Self> {
        let (Some(before), Some(after)) = (before.as_object(), after.as_object()) else {
            return Vec::new();
        };
        let dropped = before.keys().filter(|field| !after.contains_key(*field));
        after
            .keys()
            .chain(dropped)
            .filter_map(|field| {
                let old = before.get(field).unwrap_or(&Value::Null);
                let new = after.get(field).unwrap_or(&Value::Null);
                (old != new).then(|| Self {
                    field: field.clone(),
                    before: old.clone(),
                    after: new.clone(),
                })
            })
            .collect()
    }

    pub async fn insert(
        &self,
        conn: &mut SqliteConnection,
        snapshot: &Snapshot,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO record_changes (name, kind, field, before, after, snapshot_id, observed_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&snapshot.name)
        .bind(snapshot.kind)
        .bind(&self.field)
        .bind(Json(&self.before))
        .bind(Json(&self.after))
        .bind(snapshot.id)
        .bind(snapshot.observed_at.trunc_subsecs(0))
        .execute(conn)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn value_through_null_is_two_changes() {
        let first = json!({"registered": true, "registrar": "Other Registrar Inc."});
        let gap = json!({"registered": true, "registrar": null});
        let last = json!({"registered": true, "registrar": "MarkMonitor Inc."});

        assert_eq!(
            RecordChange::between(&first, &gap),
            vec![RecordChange {
                field: "registrar".into(),
                before: json!("Other Registrar Inc."),
                after: Value::Null,
            }]
        );
        assert_eq!(
            RecordChange::between(&gap, &last),
            vec![RecordChange {
                field: "registrar".into(),
                before: Value::Null,
                after: json!("MarkMonitor Inc."),
            }]
        );
    }

    #[async_std::test]
    async fn history_reads_only_its_kind_and_numeric_json_values() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::raw_sql(
            "INSERT INTO snapshots (id, name, kind, source, server, fields, raw, observed_at, confirmed_at)
                VALUES (1, 'example.com', 'registry', 'rdap', 's', '{}', '', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
             INSERT INTO record_changes (name, kind, field, before, after, snapshot_id, observed_at) VALUES
                ('example.com', 'registry', 'count', '1', '2', 1, '2026-01-01T00:00:00Z'),
                ('example.com', 'dns', 'a', '\"192.0.2.1\"', '\"192.0.2.2\"', 1, '2026-01-01T00:00:00Z');",
        )
        .execute(&pool)
        .await
        .unwrap();

        let mut conn = pool.acquire().await.unwrap();
        let logged = LoggedChange::latest_per_name(&mut conn, IndexKind::Registry, 20)
            .await
            .unwrap();

        assert_eq!(logged.len(), 1);
        assert_eq!(
            (&logged[0].field, &logged[0].before.0, &logged[0].after.0),
            (&"count".to_string(), &json!(1), &json!(2))
        );
    }
}

use chrono::SubsecRound;
use serde_json::Value;
use sqlx::{types::Json, SqliteConnection};

use crate::models::snapshot::Snapshot;

/// One changed field between two snapshots of a name. Rows are append-only, so a reader keeps a cursor on `id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordChange {
    pub field: String,
    pub before: Value,
    pub after: Value,
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
}

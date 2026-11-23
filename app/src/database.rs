use crate::state::DatabaseConfig;
use sqlx::{migrate::MigrateDatabase, SqlitePool};
use tracing::info;

pub struct Database {
    pub pool: SqlitePool,
}

impl Database {
    pub async fn init(database_config: &DatabaseConfig) -> Self {
        let database_url = database_config.url.clone().unwrap_or_else(|| {
            let config_dir = std::env::var("XDG_CONFIG_HOME").unwrap_or_else(|_| {
                std::env::var("HOME").unwrap() + "/.config"
            });
            let db_path = config_dir + "/dmn/db.sqlite";
            format!("sqlite:{}", db_path).to_string()
        });

        info!("Database URL: {}", database_url);

        // SQLX
        if !sqlx::Sqlite::database_exists(&database_url).await.unwrap() {
            sqlx::Sqlite::create_database(&database_url).await.unwrap();
        }

        let database = Self {
            pool: SqlitePool::connect(&database_url).await.unwrap(),
        };

        database.migrate().await;
        database
    }

    pub async fn migrate(&self) {
        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await
            .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use sqlx::sqlite::SqlitePoolOptions;

    #[async_std::test]
    async fn canonical_index_names_migration_keeps_history_reachable() {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        let migrator = sqlx::migrate!("./migrations");
        let (last, earlier) = migrator.migrations.split_last().unwrap();
        assert_eq!(last.description, "canonical index names");
        for migration in earlier {
            sqlx::raw_sql(&migration.sql).execute(&pool).await.unwrap();
        }
        sqlx::raw_sql(
            "INSERT INTO index_state (name, kind, next_check_at) VALUES
                ('EXAMPLE.com', 'registry', '2030-01-01'), ('Example.com.', 'registry', '2030-01-01'),
                (' Example.NET. ', 'registry', '2030-01-01'), (char(9) || 'example.org..', 'registry', '2030-01-01');
             INSERT INTO snapshots (name, kind, source, server, fields, raw, observed_at, confirmed_at) VALUES
                ('EXAMPLE.com', 'registry', 'rdap', 's', '{}', '', '2026-01-01', '2026-01-01'),
                (' Example.NET. ', 'registry', 'rdap', 's', '{}', '', '2026-01-01', '2026-01-01');
             INSERT INTO record_changes (name, kind, field, before, after, snapshot_id, observed_at) VALUES
                (' Example.NET. ', 'registry', 'registrar', 'null', '\"X\"', 2, '2026-01-01');",
        )
        .execute(&pool)
        .await
        .unwrap();

        sqlx::raw_sql(&last.sql).execute(&pool).await.unwrap();

        let names = |table: &str| {
            let query = format!("SELECT name FROM {table} ORDER BY name");
            let pool = pool.clone();
            async move {
                sqlx::query_scalar::<_, String>(&query)
                    .fetch_all(&pool)
                    .await
                    .unwrap()
            }
        };
        assert_eq!(
            names("index_state").await,
            ["example.com", "example.net", "example.org."]
        );
        assert_eq!(names("snapshots").await, ["example.com", "example.net"]);
        assert_eq!(names("record_changes").await, ["example.net"]);
    }
}

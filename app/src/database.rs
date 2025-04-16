use crate::state::DatabaseConfig;
use sqlx::{migrate::MigrateDatabase, Acquire, AnyPool, PgPool, Pool, SqlitePool};

pub struct Database {
    pub pool: SqlitePool,
}

impl Database {
    pub async fn init(database_config: &DatabaseConfig) -> Self {
        let database_url = database_config.url.clone().unwrap_or("sqlite://./sqlite.db".to_string());

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

use crate::state::DatabaseConfig;
use sqlx::{migrate::MigrateDatabase, SqlitePool};

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
            format!("sqlite://{}", db_path).to_string()
        });

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

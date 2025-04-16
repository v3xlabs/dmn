use crate::state::DatabaseConfig;
use sqlx::PgPool;

pub struct Database {
    pub pool: PgPool,
}

impl Database {
    pub async fn init(database_config: &DatabaseConfig) -> Self {
        let database = Self {
            pool: PgPool::connect(&database_config.url).await.unwrap(),
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

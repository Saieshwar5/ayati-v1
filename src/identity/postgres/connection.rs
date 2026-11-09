//! Opening Cuckoo's PostgreSQL connection pool.
use super::PgStore;
use anyhow::{Result, ensure};
use sea_orm::{ConnectOptions, Database};
use std::time::Duration;

impl PgStore {
    pub async fn connect(url: &str) -> Result<Self> {
        ensure!(
            url.starts_with("postgres://") || url.starts_with("postgresql://"),
            "Use a PostgreSQL database URL"
        );
        let mut options = ConnectOptions::new(url);
        options
            .max_connections(10)
            .min_connections(1)
            .connect_timeout(Duration::from_secs(10))
            .acquire_timeout(Duration::from_secs(10))
            .sqlx_logging(false);
        // Driver errors may contain connection details. Do not propagate them to logs/UI.
        let db = Database::connect(options)
            .await
            .map_err(|_| anyhow::anyhow!("Cannot connect to Cuckoo PostgreSQL"))?;
        Ok(Self::with(db))
    }
}

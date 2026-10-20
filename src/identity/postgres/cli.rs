use super::*;
use std::path::Path;

/// Explicit migrations/imports, never run destructive schema synchronization on API startup.
pub async fn run(data: &Path, action: &str) -> Result<()> {
    let url = std::env::var("RELAY_DATABASE_URL").context("Set RELAY_DATABASE_URL privately")?;
    let db = PgStore::connect(&url).await?;
    match action {
        "migrate" => {
            db.migrate().await?;
            println!("PostgreSQL workspace schema is current.");
        }
        "check" => {
            db.verify_schema().await?;
            println!("PostgreSQL workspace schema is ready.");
        }
        "import" => {
            let root = data.canonicalize()?.join("identity");
            let _lock = crate::identity::adoption::lock(&root.join("server.lock"))?;
            let _daemon = crate::identity::adoption::lock(&data.join("daemon.lock"))?;
            db.verify_schema().await?;
            // Fence SQLite before committing PostgreSQL: a crash between these operations
            // must never silently resume writes against the old database.
            use std::io::Write;
            let mut fence = std::fs::OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(root.join("postgres-identity"))?;
            fence.write_all(b"PostgreSQL identity cutover started. Verify import and configure RELAY_DATABASE_URL before restart.\n")?;
            fence.sync_all()?;
            std::fs::File::open(&root)?.sync_all()?;
            let counts = db.import_sqlite(&root.join("identity.sqlite")).await?;
            println!(
                "Identity import verified: {counts}. Sign in again; existing business files remain in place."
            );
        }
        _ => anyhow::bail!("Use workspace-db migrate, check or import"),
    }
    Ok(())
}
impl PgStore {
    pub async fn verify_import(&self) -> Result<()> {
        ensure!(
            query(
                &self.db,
                "SELECT source_id FROM relay_imports WHERE source_id='sqlite-identity-v1'",
                vec![]
            )
            .await?
            .is_some(),
            "Complete workspace-db import before starting against existing SQLite identity data"
        );
        Ok(())
    }

    pub async fn verify_schema(&self) -> Result<()> {
        let applied = rows(
            &self.db,
            "SELECT version FROM seaql_migrations ORDER BY version",
            vec![],
        )
        .await
        .map_err(|_| {
            anyhow::anyhow!("Run workspace-db migrate before starting Relay PostgreSQL")
        })?;
        ensure!(
            applied.len() == 2
                && applied[0].try_get::<String>("", "version")?
                    == "m20261020_000001_workspace_foundation"
                && applied[1].try_get::<String>("", "version")?
                    == "m20261020_000002_conversation_kinds",
            "Run workspace-db migrate: unsupported workspace schema version"
        );
        Ok(())
    }
}

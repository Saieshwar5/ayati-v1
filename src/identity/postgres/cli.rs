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
            anyhow::anyhow!("Run workspace-db migrate before starting Cuckoo PostgreSQL")
        })?;
        ensure!(
            applied.len() == 20
                && applied[0].try_get::<String>("", "version")?
                    == "m20261020_000001_workspace_foundation"
                && applied[1].try_get::<String>("", "version")?
                    == "m20261020_000002_conversation_kinds"
                && applied[2].try_get::<String>("", "version")? == "m20261021_000003_display_names"
                && applied[3].try_get::<String>("", "version")?
                    == "m20261021_000004_company_collaboration"
                && applied[4].try_get::<String>("", "version")?
                    == "m20261021_000005_queue_telemetry"
                && applied[5].try_get::<String>("", "version")?
                    == "m20261021_000006_reliable_messaging"
                && applied[6].try_get::<String>("", "version")?
                    == "m20261021_000007_agent_profiles"
                && applied[7].try_get::<String>("", "version")?
                    == "m20261022_000008_workspace_roles"
                && applied[8].try_get::<String>("", "version")? == "m20261022_000009_library"
                && applied[9].try_get::<String>("", "version")? == "m20261023_000010_login_flows"
                && applied[10].try_get::<String>("", "version")?
                    == "m20261023_000011_worker_assignments"
                && applied[11].try_get::<String>("", "version")?
                    == "m20261023_000012_worker_control"
                && applied[12].try_get::<String>("", "version")?
                    == "m20261023_000013_executor_slots"
                && applied[13].try_get::<String>("", "version")? == "m20261023_000014_reply_output"
                && applied[14].try_get::<String>("", "version")?
                    == "m20261023_000015_harness_preparation"
                && applied[15].try_get::<String>("", "version")?
                    == "m20261024_000016_interactive_reserve"
                && applied[16].try_get::<String>("", "version")?
                    == "m20261024_000017_cloud_executors"
                && applied[17].try_get::<String>("", "version")?
                    == "m20261025_000018_shared_control"
                && applied[18].try_get::<String>("", "version")?
                    == "m20261025_000019_connection_authority"
                && applied[19].try_get::<String>("", "version")?
                    == "m20261025_000020_guided_permissions",
            "Run workspace-db migrate: unsupported workspace schema version"
        );
        Ok(())
    }
}

//! Schema 132: an app connection can belong to one agent, and schema 133: every
//! one does (the workspace's shared connections end). The rules live with
//! the code they change: who may connect (`workspace_connections::authority`),
//! the connect and sign-in flow, the agent's tool list (`agent_tools.sql`) and
//! the per-action rules (`connector_rules.rs`).
#[cfg(test)]
#[path = "connection_owners_tests.rs"]
mod tests;

/// For upgrade tests written when connections were the workspace's: migrates
/// to the current schema with each such connection given to the one agent that
/// uses it, as its owner would by connecting it again, so schema 133 leaves it
/// working and the test still checks what it was written for.
#[cfg(test)]
pub(super) async fn migrate_keeping_connections(db: &super::PgStore) {
    use sea_orm::ConnectionTrait;
    use sea_orm_migration::MigratorTrait;
    let all = super::migration::Migrator::migrations();
    let end = all
        .iter()
        .position(|m| m.name() == "m20261118_000133_shared_connections_ended")
        .expect("schema 133 is registered");
    let applied = super::query(&db.db, "SELECT count(*) AS n FROM seaql_migrations", vec![])
        .await
        .unwrap()
        .unwrap()
        .try_get::<i64>("", "n")
        .unwrap() as usize;
    if applied < end {
        super::migration::Migrator::up(&db.db, Some((end - applied) as u32))
            .await
            .unwrap();
        db.db
            .execute_unprepared(
                "UPDATE relay_control_connections c SET agent=(SELECT min(ac.agent) FROM relay_agent_connections ac
                   WHERE ac.workspace=c.workspace AND ac.connection=c.id)
                 WHERE c.agent IS NULL AND c.provider<>'system'",
            )
            .await
            .unwrap();
    }
    db.migrate().await.unwrap();
}

pub(crate) use migration::{ConnectionOwners, SharedConnectionsEnded};
mod migration {
    use sea_orm_migration::prelude::*;
    pub(crate) struct ConnectionOwners;
    impl MigrationName for ConnectionOwners {
        fn name(&self) -> &str {
            "m20261118_000132_connection_owners"
        }
    }
    #[async_trait::async_trait]
    impl MigrationTrait for ConnectionOwners {
        async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
            m.get_connection()
                .execute_unprepared(include_str!("connection_owners.sql"))
                .await?;
            Ok(())
        }
        async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
            Err(DbErr::Custom(
                "Connection owners require a forward migration".into(),
            ))
        }
    }
    /// Schema 133: the workspace's shared connections end; every connection
    /// belongs to an agent.
    pub(crate) struct SharedConnectionsEnded;
    impl MigrationName for SharedConnectionsEnded {
        fn name(&self) -> &str {
            "m20261118_000133_shared_connections_ended"
        }
    }
    #[async_trait::async_trait]
    impl MigrationTrait for SharedConnectionsEnded {
        async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
            m.get_connection()
                .execute_unprepared(include_str!("shared_connections_ended.sql"))
                .await?;
            Ok(())
        }
        async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
            Err(DbErr::Custom(
                "Ending shared connections requires a forward migration".into(),
            ))
        }
    }
}

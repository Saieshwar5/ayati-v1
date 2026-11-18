//! Schema 132: an app connection can belong to one agent. The rules live with
//! the code they change: who may connect (`workspace_connections::authority`),
//! the connect and sign-in flow, the agent's tool list (`agent_tools.sql`) and
//! the per-action rules (`connector_rules.rs`).
#[cfg(test)]
#[path = "connection_owners_tests.rs"]
mod tests;

pub(crate) use migration::ConnectionOwners;
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
}

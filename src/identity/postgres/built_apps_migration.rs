//! Built apps' schema history. Built apps (schemas 78–86) were removed in schema 126;
//! their migrations stay so existing databases keep a complete, ordered history and
//! new ones are built the same way before schema 126 drops the tables.
use sea_orm::ConnectionTrait;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};

/// One forward-only migration that runs a fixed SQL script.
pub(crate) struct SqlMigration {
    name: &'static str,
    sql: &'static str,
    down: &'static str,
}
impl MigrationName for SqlMigration {
    fn name(&self) -> &str {
        self.name
    }
}
#[async_trait::async_trait]
impl MigrationTrait for SqlMigration {
    async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
        m.get_connection().execute_unprepared(self.sql).await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(self.down.into()))
    }
}

/// Schema 78: built apps and their versions' files.
pub(crate) const BUILT_APPS: SqlMigration = SqlMigration {
    name: "m20261105_000078_built_apps",
    sql: include_str!("built_apps.sql"),
    down: "Built apps require a forward migration",
};
/// Schema 79: built apps' records and roles.
pub(crate) const BUILT_APP_DATA: SqlMigration = SqlMigration {
    name: "m20261105_000079_built_app_data",
    sql: include_str!("built_app_data.sql"),
    down: "Built app data requires a forward migration",
};
/// Schema 80: agents asked to build apps.
pub(crate) const BUILT_APP_AGENTS: SqlMigration = SqlMigration {
    name: "m20261105_000080_built_app_agents",
    sql: include_str!("built_app_agents.sql"),
    down: "Built app agents require a forward migration",
};
/// Schema 81: which app actions agents may run, and the runs.
pub(crate) const BUILT_APP_ACTIONS: SqlMigration = SqlMigration {
    name: "m20261105_000081_built_app_actions",
    sql: include_str!("built_app_actions.sql"),
    down: "Built app actions require a forward migration",
};
/// Schema 84: app event watchers and the events waiting for them.
pub(crate) const BUILT_APP_EVENTS: SqlMigration = SqlMigration {
    name: "m20261105_000084_built_app_events",
    sql: include_str!("built_app_events.sql"),
    down: "Built app events require a forward migration",
};
/// Schema 85: app action rules may ask the person first.
pub(crate) const BUILT_APP_ASKING: SqlMigration = SqlMigration {
    name: "m20261105_000085_built_app_asking",
    sql: "ALTER TABLE relay_built_app_agent_rules DROP CONSTRAINT relay_built_app_agent_rules_mode_check;
          ALTER TABLE relay_built_app_agent_rules ADD CONSTRAINT relay_built_app_agent_rules_mode_check CHECK(mode IN ('allow','ask','block'));",
    down: "Asking first requires a forward migration",
};
/// Schema 86: watchers for changed records, and preview errors.
pub(crate) const BUILT_APP_CHANGES: SqlMigration = SqlMigration {
    name: "m20261105_000086_built_app_changes",
    sql: include_str!("built_app_changes.sql"),
    down: "Built app changes require a forward migration",
};
/// Schema 126: built apps removed. Their tables, records and waiting events go.
pub(crate) const BUILT_APPS_REMOVED: SqlMigration = SqlMigration {
    name: "m20261117_000126_built_apps_removed",
    sql: include_str!("built_apps_removed.sql"),
    down: "Removing built apps requires a forward migration",
};

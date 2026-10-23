//! PostgreSQL identity storage. All access stays behind the existing Cuckoo API.
mod accounts;
mod agent_knowledge;
mod agent_profiles;
#[cfg(test)]
mod assignment_tests;
mod assignments;
mod chat;
pub mod cli;
mod collaboration;
#[cfg(test)]
mod controlled_attempt_tests;
mod controlled_attempts;
mod executor_dispatch;
#[cfg(test)]
mod executor_slot_tests;
mod executor_slots;
pub(crate) use executor_dispatch::PoolLimits;
mod entities;
mod feedback;
mod history;
mod import;
mod jobs;
mod library;
#[cfg(test)]
mod library_http_tests;
#[cfg(test)]
mod library_tests;
mod login_flows;
mod messaging;
pub mod migration;
mod objects;
mod people;
mod resources;
mod settings;
mod work;
pub(crate) mod worker_control;
#[cfg(test)]
mod worker_control_migration_tests;

use super::store::{User, now};
use anyhow::{Context, Result, ensure};
use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DatabaseTransaction,
    QueryResult, Statement, TransactionTrait, Value as SqlValue,
};
use serde_json::Value;
use std::time::Duration;

pub struct PgStore {
    db: DatabaseConnection,
    pub(crate) changes: tokio::sync::broadcast::Sender<String>,
    pub(crate) assigned: bool,
    pub(crate) pooled: bool,
    pub(crate) assignment_seconds: u64,
}
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
        Ok(Self {
            db,
            changes: tokio::sync::broadcast::channel(256).0,
            assigned: super::worker_config::Mode::from_env(true)?
                == super::worker_config::Mode::Assigned,
            pooled: super::worker_config::Mode::from_env(true)?
                == super::worker_config::Mode::Pooled,
            assignment_seconds: super::worker_config::seconds(
                "AYATI_ASSIGNMENT_RUN_SECONDS",
                180,
                1,
                86400,
            )?,
        })
    }

    /// Serialize mutations only within a workspace. Independent businesses use other connections.
    async fn workspace_tx(&self, workspace: &str) -> Result<DatabaseTransaction> {
        let tx = self.db.begin().await?;
        query(
            &tx,
            "SELECT id FROM relay_workspaces WHERE id=$1 FOR UPDATE",
            vec![workspace.into()],
        )
        .await?
        .context("Workspace access denied")?;
        Ok(tx)
    }

    async fn read_tx(&self, workspace: &str) -> Result<DatabaseTransaction> {
        let tx = self.db.begin().await?;
        query(
            &tx,
            "SELECT id FROM relay_workspaces WHERE id=$1 FOR SHARE",
            vec![workspace.into()],
        )
        .await?
        .context("Workspace access denied")?;
        Ok(tx)
    }

    pub async fn role(&self, workspace: &str, actor: &str) -> Result<String> {
        role(&self.db, workspace, actor).await
    }
}
fn statement(sql: &str, values: Vec<SqlValue>) -> Statement {
    Statement::from_sql_and_values(sea_orm::DbBackend::Postgres, sql, values)
}
async fn execute(db: &impl ConnectionTrait, sql: &str, values: Vec<SqlValue>) -> Result<u64> {
    Ok(db
        .execute_raw(statement(sql, values))
        .await?
        .rows_affected())
}
async fn query(
    db: &impl ConnectionTrait,
    sql: &str,
    values: Vec<SqlValue>,
) -> Result<Option<QueryResult>> {
    Ok(db.query_one_raw(statement(sql, values)).await?)
}
async fn rows(
    db: &impl ConnectionTrait,
    sql: &str,
    values: Vec<SqlValue>,
) -> Result<Vec<QueryResult>> {
    Ok(db.query_all_raw(statement(sql, values)).await?)
}
async fn json_rows(
    db: &impl ConnectionTrait,
    sql: &str,
    values: Vec<SqlValue>,
) -> Result<Vec<Value>> {
    rows(db, sql, values)
        .await?
        .into_iter()
        .map(|r| Ok(r.try_get("", "data")?))
        .collect()
}
async fn role(db: &impl ConnectionTrait, workspace: &str, actor: &str) -> Result<String> {
    query(
        db,
        "SELECT role FROM relay_members WHERE workspace=$1 AND user_id=$2",
        vec![workspace.into(), actor.into()],
    )
    .await?
    .context("Workspace access denied")?
    .try_get("", "role")
    .map_err(Into::into)
}
async fn manager(db: &impl ConnectionTrait, workspace: &str, actor: &str) -> Result<String> {
    let role = role(db, workspace, actor).await?;
    ensure!(role == "Owner", "Only Owners can manage workspace access");
    Ok(role)
}
async fn audit(
    db: &impl ConnectionTrait,
    actor: &str,
    workspace: Option<&str>,
    action: &str,
    target: &str,
) -> Result<()> {
    execute(
        db,
        "INSERT INTO relay_audit(actor,workspace,action,target,at) VALUES($1,$2,$3,$4,$5)",
        vec![
            actor.into(),
            workspace.into(),
            action.into(),
            target.into(),
            now().into(),
        ],
    )
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod profile_tests;

#[cfg(test)]
mod company_tests;

#[cfg(test)]
mod messaging_tests;
pub(crate) use agent_knowledge::KnowledgeInput;
#[cfg(test)]
mod agent_profile_tests;

#[cfg(test)]
mod ownership_tests;

#[cfg(test)]
mod workspace_scenario_tests;

#[cfg(test)]
mod hosting_tests;
#[cfg(test)]
mod worker_tests;

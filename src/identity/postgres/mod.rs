//! PostgreSQL identity storage. All access stays behind the existing Cuckoo API.
mod accounts;
mod agent_knowledge;
mod agent_profiles;
mod agent_receipt_migration;
#[cfg(test)]
mod agent_receipt_tests;
mod agent_receipts;
#[cfg(test)]
mod agent_request_tests;
mod agent_requests;
mod approval_authority;
mod approval_delivery;
#[cfg(test)]
mod approval_tests;
mod assignments;
mod attempt_resume;
#[cfg(test)]
mod attempt_resume_tests;
mod attention;
mod attention_migration;
#[cfg(test)]
mod attention_migration_tests;
#[cfg(test)]
mod attention_scope_tests;
#[cfg(test)]
mod attention_tests;
mod automation_access;
mod automation_dispatch;
mod automation_migration;
#[cfg(test)]
mod automation_migration_tests;
#[cfg(test)]
mod automation_scope_tests;
#[cfg(test)]
mod automation_tests;
mod automations;
#[cfg(test)]
mod channel_archive_recovery_tests;
mod channel_lifecycle;
mod chat;
mod chat_intake;
mod chat_notification_migration;
#[cfg(test)]
mod chat_notification_tests;
mod chat_notifications;
mod chat_organization_migration;
#[cfg(test)]
mod chat_organization_migration_tests;
#[cfg(test)]
mod chat_organization_tests;
mod chat_people_migration;
#[cfg(test)]
mod chat_people_tests;
mod chat_pins;
pub mod cli;
#[cfg(test)]
mod cloud_executor_tests;
mod cloud_executors;
mod collaboration;
mod communication_access;
mod communication_endpoints;
mod communication_inbox;
mod communication_migration;
mod communication_outbox;
#[cfg(test)]
mod communication_recovery_tests;
#[cfg(test)]
mod communication_tests;
mod connection_authority;
#[cfg(test)]
mod connection_authority_tests;
#[cfg(test)]
mod connection_migration_tests;
pub(crate) mod control_actions;
mod control_rules;
#[cfg(test)]
mod control_tests;
#[cfg(test)]
mod controlled_attempt_tests;
mod controlled_attempts;
mod data_lifecycle;
mod data_lifecycle_migration;
#[cfg(test)]
mod data_lifecycle_tests;
#[cfg(test)]
mod dispatch_lock_tests;
mod durable_wait_migration;
mod executor_dispatch;
#[cfg(test)]
mod executor_slot_tests;
mod executor_slots;
mod executor_task_migration;
mod executor_tasks;
pub mod member_profiles;
mod message_action_migration;
#[cfg(test)]
mod message_action_tests;
mod message_actions;
mod message_reply_migration;
#[cfg(test)]
mod message_reply_tests;
mod pool_metrics;
#[cfg(test)]
mod pool_metrics_tests;
pub(crate) use pool_metrics::PoolSnapshot;
#[cfg(test)]
mod preparation_tests;
#[cfg(test)]
mod profile_lookup_tests;
mod question_migration;
#[cfg(test)]
mod question_migration_tests;
#[cfg(test)]
mod question_source_tests;
#[cfg(test)]
mod question_tests;
mod question_views;
mod questions;
mod task_progress;
#[cfg(test)]
mod task_progress_tests;
mod workspace_connections;
pub(crate) use executor_dispatch::PoolLimits;
mod context_attempts;
mod context_migration;
mod context_preview;
#[cfg(test)]
mod context_preview_tests;
#[cfg(test)]
mod context_version_tests;
mod conversation_context;
#[cfg(test)]
mod conversation_context_tests;
mod conversation_history;
#[cfg(test)]
mod conversation_history_source_tests;
#[cfg(test)]
mod conversation_history_tests;
mod entities;
mod event_access;
#[cfg(test)]
mod event_authority_tests;
mod event_dispatch;
mod event_intake;
mod event_migration;
#[cfg(test)]
mod event_migration_tests;
#[cfg(test)]
mod event_scope_tests;
#[cfg(test)]
mod event_tests;
mod event_triggers;
#[cfg(test)]
mod executor_task_tests;
mod feedback;
mod handoff_access;
mod handoff_admission;
#[cfg(test)]
mod handoff_authority_tests;
mod handoff_controls;
mod handoff_lifecycle;
mod handoff_migration;
#[cfg(test)]
mod handoff_migration_tests;
#[cfg(test)]
mod handoff_source_tests;
#[cfg(test)]
mod handoff_tests;
mod handoff_views;
mod history;
#[cfg(test)]
mod home_general_tests;
mod home_health;
mod home_maintenance;
mod home_maintenance_migration;
#[cfg(test)]
mod home_maintenance_scope_tests;
#[cfg(test)]
mod home_maintenance_tests;
mod home_migration;
#[cfg(test)]
mod home_migration_tests;
#[cfg(test)]
mod home_panel_tests;
mod home_panels;
mod home_receipts;
mod home_refresh;
#[cfg(test)]
mod home_source_tests;
mod home_sources;
mod home_update_access;
mod home_update_runs;
mod home_views;
#[cfg(test)]
mod interactive_reserve_tests;
mod jobs;
mod library;
#[cfg(test)]
mod library_http_tests;
#[cfg(test)]
mod library_tests;
mod login_flows;
mod messaging;
pub mod migration;
#[cfg(test)]
mod migration_fixture;
mod objects;
mod people;
mod rehearsal_context;
mod rehearsal_history;
mod rehearsal_migration;
#[cfg(test)]
mod rehearsal_migration_tests;
mod rehearsal_queue;
#[cfg(test)]
mod rehearsal_source_tests;
#[cfg(test)]
mod rehearsal_tests;
mod rehearsals;
mod reply_output;
#[cfg(test)]
mod reply_output_tests;
mod reply_views;
mod request_context;
#[cfg(test)]
mod request_context_tests;
#[cfg(test)]
mod response_tests;
mod responses;
mod settings;
mod setup_context;
mod setup_drafts;
#[cfg(test)]
mod setup_suggestion_tests;
mod setup_suggestions;
#[cfg(test)]
mod setup_tests;
mod source_access;
mod task_continuation;
#[cfg(test)]
mod task_continuation_tests;
mod task_revocation;
mod work;
pub(crate) mod worker_control;
#[cfg(test)]
mod worker_control_migration_tests;
mod working_context;

use super::user::{User, now};
use anyhow::{Context, Result, ensure};
use sea_orm::{
    ConnectOptions, ConnectionTrait, Database, DatabaseConnection, DatabaseTransaction,
    QueryResult, Statement, TransactionTrait, Value as SqlValue,
};
use serde_json::Value;
use std::time::Duration;

#[derive(Clone)]
pub struct PgStore {
    db: DatabaseConnection,
    pub(crate) changes: tokio::sync::broadcast::Sender<String>,
    /// Agent replies go to pooled executors. False only for the scripted
    /// company simulation, which claims polling jobs inside the controller.
    pub(crate) pooled: bool,
    pub(crate) assignment_seconds: u64,
    pub(crate) handoff_wait_seconds: i64,
}
impl PgStore {
    /// A store whose every query fails, for HTTP tests that must reject a
    /// request before it reaches PostgreSQL.
    #[cfg(test)]
    pub(crate) fn disconnected() -> Self {
        Self {
            db: DatabaseConnection::default(),
            changes: tokio::sync::broadcast::channel(256).0,
            pooled: false,
            assignment_seconds: 180,
            handoff_wait_seconds: 604_800,
        }
    }
    /// Connect with the controller's execution and store policy, then verify the schema.
    pub(crate) async fn open(config: &crate::config::Controller) -> Result<Self> {
        let mut store = Self::connect(&config.database_url).await?;
        store.pooled = config.execution == crate::config::Execution::Pooled;
        store.assignment_seconds = config.pool.run_seconds;
        store.handoff_wait_seconds = config.handoff_wait_seconds;
        store.verify_schema().await?;
        Ok(store)
    }
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
            pooled: false,
            assignment_seconds: 180,
            handoff_wait_seconds: 604_800,
        })
    }

    /// Serialize mutations only within a workspace. Independent businesses use other connections.
    async fn workspace_tx(&self, workspace: &str) -> Result<DatabaseTransaction> {
        let tx = self.db.begin().await?;
        query(
            &tx,
            // Writers serialize without blocking FK KEY SHARE locks on change
            // records. FOR UPDATE creates a workspace -> sync-clock -> workspace
            // cycle when message intake overlaps assignment projection writes.
            "SELECT id FROM relay_workspaces WHERE id=$1 FOR NO KEY UPDATE",
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
pub(in crate::identity) mod tests;

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
mod gateway_tests;
#[cfg(test)]
mod hosting_tests;
#[cfg(test)]
mod worker_tests;

//! PostgreSQL identity storage. All access stays behind the existing Cuckoo API.
mod accounts;
pub(crate) mod activity;
mod activity_migration;
mod activity_unhandled;
mod admin_member_migration;
#[cfg(test)]
mod admin_member_migration_tests;
mod agent_audience;
mod agent_audience_migration;
#[cfg(test)]
mod agent_audience_tests;
mod agent_changes;
mod agent_instructions_migration;
#[cfg(test)]
mod agent_instructions_tests;
mod agent_knowledge;
mod agent_pause_switch;
mod agent_profiles;
mod agent_receipt_migration;
#[cfg(test)]
mod agent_receipt_tests;
mod agent_receipts;
#[cfg(test)]
mod agent_request_tests;
mod agent_requests;
#[cfg(test)]
mod agent_tools_tests;
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
mod channel_approval_migration;
#[cfg(test)]
mod channel_archive_recovery_tests;
mod channel_lifecycle;
pub(crate) mod chat;
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
mod communications;
mod connection;
#[cfg(test)]
mod connection_migration_tests;
pub(crate) mod connector_calls;
pub(crate) mod connector_providers;
pub(crate) mod connector_routing;
mod connector_rules;
#[cfg(test)]
mod connector_rules_tests;
mod connector_safety_migration;
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
#[cfg(test)]
mod model_budget_tests;
pub(crate) mod model_budgets;
#[cfg(test)]
mod model_catalog_tests;
pub(crate) mod model_operations;
#[cfg(test)]
mod model_operations_tests;
#[cfg(test)]
mod model_usage_report_tests;
pub(crate) mod model_usage_reports;
mod pool_metrics;
#[cfg(test)]
mod pool_metrics_tests;
#[cfg(test)]
mod tool_test_support;
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
pub(crate) mod workspace_connections;
pub(crate) use executor_dispatch::PoolLimits;
mod browser_tools;
mod computer_browser_migration;
#[cfg(test)]
mod computer_browser_tests;
mod computer_live;
mod computer_policy;
#[cfg(test)]
mod computer_policy_tests;
#[cfg(test)]
mod computer_turn_tests;
mod computer_turns;
pub(crate) use computer_live::LiveInput;
#[cfg(test)]
mod computer_tests;
mod computer_tools;
mod computers;
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
pub(crate) mod event_subscriptions;
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
mod home_frame_migration;
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
pub(crate) mod invitation_delivery;
mod invite_link_migration;
#[cfg(test)]
mod invite_link_tests;
pub(crate) mod invite_links;
mod jobs;
mod library;
#[cfg(test)]
mod library_http_tests;
#[cfg(test)]
mod library_tests;
mod login_flows;
pub(crate) mod message_search;
#[cfg(test)]
mod message_search_tests;
pub(crate) mod messaging;
pub mod migration;
#[cfg(test)]
mod migration_fixture;
mod objects;
pub(crate) mod operator;
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
mod request_retry;
#[cfg(test)]
mod response_tests;
mod responses;
pub(crate) mod settings;
mod shared_rooms;
mod skill_writing;
#[cfg(test)]
mod skill_writing_tests;
pub(crate) mod skills;
mod source_access;
mod system_prompt;
#[cfg(test)]
mod system_prompt_tests;
mod task_continuation;
#[cfg(test)]
mod task_continuation_tests;
mod task_revocation;
mod tasks;
pub(crate) mod transcripts;
mod unread;
mod web_reads;
mod work;
mod work_rooms;
pub(crate) mod work_signals;
pub(crate) mod worker_control;
#[cfg(test)]
mod worker_control_migration_tests;
mod working_context;
mod workspace_code_migration;
#[cfg(test)]
mod workspace_code_tests;
pub(crate) mod workspace_codes;

use super::user::{User, now};
use anyhow::{Context, Result, ensure};
use sea_orm::{
    ConnectionTrait, DatabaseConnection, DatabaseTransaction, QueryResult, TransactionTrait,
    Value as SqlValue,
};
use serde_json::Value;
use std::time::Duration;

#[derive(Clone)]
pub struct PgStore {
    db: DatabaseConnection,
    pub(crate) changes: tokio::sync::broadcast::Sender<messaging::Live>,
    /// Wake-ups for the background loops, from the same listener (schema 97).
    pub(crate) work: std::sync::Arc<work_signals::WorkSignals>,
    /// Agent replies go to pooled executors. False only for the scripted
    /// company simulation, which claims polling jobs inside the controller.
    pub(crate) pooled: bool,
    pub(crate) assignment_seconds: u64,
    pub(crate) handoff_wait_seconds: i64,
    /// Library embedding and summary models (LB2b), when configured.
    pub(crate) library_models: Option<crate::identity::library::models::Configured>,
    /// The model catalog (MG1), when configured: agents then choose catalog models.
    pub(crate) model_catalog: Option<crate::identity::models::catalog::Files>,
    /// Web search services for agents (schema 102), when configured.
    pub(crate) web_search: Option<crate::identity::web_search::WebSearch>,
    /// Agent computers (schema 103), when configured.
    pub(crate) computers: Option<crate::identity::computers::Computers>,
}
impl PgStore {
    /// A store whose every query fails, for HTTP tests that must reject a
    /// request before it reaches PostgreSQL.
    #[cfg(test)]
    pub(crate) fn disconnected() -> Self {
        Self::with(DatabaseConnection::default())
    }
    /// The default policy; `open` applies the controller's.
    fn with(db: DatabaseConnection) -> Self {
        Self {
            db,
            changes: tokio::sync::broadcast::channel(256).0,
            work: Default::default(),
            pooled: false,
            assignment_seconds: 180,
            handoff_wait_seconds: 604_800,
            library_models: None,
            model_catalog: None,
            web_search: None,
            computers: None,
        }
    }
    /// Connect with the controller's execution and store policy, then verify the schema.
    pub(crate) async fn open(config: &crate::config::Controller) -> Result<Self> {
        let mut store = Self::connect(&config.database_url).await?;
        store.pooled = config.execution == crate::config::Execution::Pooled;
        store.assignment_seconds = config.pool.run_seconds;
        store.handoff_wait_seconds = config.handoff_wait_seconds;
        store.library_models = crate::identity::library::models::Configured::from(config);
        store.model_catalog = crate::identity::models::catalog::Files::from(config);
        store.web_search = crate::identity::web_search::WebSearch::from(config)?;
        store.computers = crate::identity::computers::Computers::from(config)?;
        store.verify_schema().await?;
        Ok(store)
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
async fn role(db: &impl ConnectionTrait, workspace: &str, actor: &str) -> Result<String> {
    query(
        db,
        // Old Owner/User names exist only in upgrading databases (schema 43). A closed
        // (deleted) or suspended (schema 87) workspace refuses everyone; to_jsonb
        // also works on older schemas.
        "SELECT CASE m.role WHEN 'Owner' THEN 'Admin' WHEN 'User' THEN 'Member' ELSE m.role END AS role FROM relay_members m JOIN relay_workspaces w ON w.id=m.workspace WHERE m.workspace=$1 AND m.user_id=$2 AND to_jsonb(w)->>'closed_ms' IS NULL AND to_jsonb(w)->>'suspended_ms' IS NULL",
        vec![workspace.into(), actor.into()],
    )
    .await?
    .context("Workspace access denied")?
    .try_get("", "role")
    .map_err(Into::into)
}
async fn manager(db: &impl ConnectionTrait, workspace: &str, actor: &str) -> Result<String> {
    let role = role(db, workspace, actor).await?;
    ensure!(role == "Admin", "Only admins can manage workspace access");
    Ok(role)
}
mod audit_write;
mod built_apps;
mod statements;
use statements::{execute, json_rows, query, rows};
mod company_apps;
use audit_write::{audit, audit_with_summary};
pub(crate) use built_apps::{
    AgentRule, AppFile, AppRelease, AppWatch, DataRequest, Helper, NewApp, PreviewError, RoleChange,
};
pub(crate) use company_apps::{AppDecision, AppRequest, CompanyApp};

#[cfg(test)]
pub(in crate::identity) mod tests;

#[cfg(test)]
mod profile_tests;

#[cfg(test)]
mod company_tests;

#[cfg(test)]
mod messaging_tests;
pub(crate) use agent_audience::TransferAgent;
pub(crate) use agent_knowledge::KnowledgeInput;
pub(crate) use communications::{
    NoFreeNumber, Offer, TelegramCreated, TelegramFrom, TelegramIncoming, TelegramLinked,
    TelegramNotConnected, TelegramOutcome, TelegramOwnerAsk, WhatsappIncoming, WhatsappOutcome,
    telegram_start_code,
};
pub(crate) use library::always::AlwaysRead;
pub(crate) use library::answers::SaveAnswer;
pub(crate) use library::edits::Decision;
pub(crate) use library::keeping::{Hold, Keeping};
pub(crate) use library::tables::SaveTable;
pub(crate) use library::upkeep::{GapAction, Review};
pub(crate) use library::watching::Watch;
pub(crate) use worker_control::model_usage::{
    ModelCall, micro_dollars as model_usage_micro_dollars,
};
#[cfg(test)]
mod agent_profile_tests;
mod audit_summary_migration;

#[cfg(test)]
mod ownership_tests;

#[cfg(test)]
mod workspace_scenario_tests;

#[cfg(test)]
mod gateway_tests;
#[cfg(test)]
mod hosting_tests;
mod member_handover;
#[cfg(test)]
mod member_handover_tests;
#[cfg(test)]
mod worker_tests;

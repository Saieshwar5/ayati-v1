use sea_orm_migration::prelude::*;
#[path = "migration_chat.rs"]
mod chat;
use chat::*;

pub struct Migrator;
#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(Foundation),
            Box::new(ConversationKinds),
            Box::new(DisplayNames),
            Box::new(Collaboration),
            Box::new(QueueTelemetry),
            Box::new(Messaging),
            Box::new(AgentProfiles),
            Box::new(WorkspaceRoles),
            Box::new(Library),
            Box::new(LoginFlows),
            Box::new(WorkerAssignments),
            Box::new(WorkerControl),
            Box::new(ExecutorSlots),
            Box::new(ReplyOutput),
            Box::new(HarnessPreparation),
            Box::new(InteractiveReserve),
            Box::new(CloudExecutors),
            Box::new(SharedControl),
            Box::new(ConnectionAuthority),
            Box::new(GuidedPermissions),
            Box::new(super::channel_approval_migration::ChannelApprovals),
            Box::new(super::durable_wait_migration::DurableWaits),
            Box::new(super::question_migration::Questions),
            Box::new(super::chat_organization_migration::ChatOrganization),
            Box::new(super::attention_migration::Attention),
            Box::new(super::rehearsal_migration::Rehearsals),
            Box::new(super::automation_migration::Automations),
            Box::new(super::event_migration::Events),
            Box::new(super::handoff_migration::Handoffs),
            Box::new(super::home_migration::HomePanels),
            Box::new(super::context_migration::ConversationContext),
            Box::new(super::communications::migration::Communications),
            Box::new(super::home_maintenance_migration::HomeMaintenance),
            Box::new(super::executor_task_migration::ExecutorTasks),
            Box::new(super::data_lifecycle_migration::DataLifecycle),
            Box::new(super::chat_people_migration::ChatPeople),
            Box::new(super::message_action_migration::MessageActions),
            Box::new(super::chat_notification_migration::ChatNotifications),
            Box::new(super::agent_receipt_migration::AgentReceipts),
            Box::new(super::message_reply_migration::MessageReplies),
            Box::new(super::workspace_code_migration::WorkspaceCodes),
            Box::new(super::invite_link_migration::InviteLinks),
            Box::new(super::admin_member_migration::AdminMemberRoles),
            Box::new(super::audit_summary_migration::AuditSummaries),
            Box::new(super::agent_audience_migration::AgentAudience),
            Box::new(super::agent_instructions_migration::AgentInstructions),
            Box::new(super::connector_safety_migration::ConnectorSafety),
            Box::new(super::activity_migration::Activity),
            Box::new(super::settings::workspace_migration::WorkspaceSettings),
            Box::new(super::skills::migration::Skills),
            Box::new(super::settings::pause_migration::AgentPause),
            Box::new(super::chat::updates_migration::AgentUpdates),
            Box::new(super::connector_providers::migration::ConnectorProviders),
            Box::new(super::communications::migration::CommunicationGateway),
            Box::new(super::shared_rooms::migration::SharedRooms),
            Box::new(super::work_rooms::migration::WorkRooms),
            Box::new(super::home_frame_migration::HomeFrame),
            Box::new(super::tasks::migration::Tasks),
            Box::new(super::home_update_runs::living::migration::HomeLiving),
            Box::new(super::home_views::team::migration::HomeTeam),
            Box::new(super::activity::now::fixing::migration::Supervising),
            Box::new(super::activity::now::steering::migration::Steering),
            Box::new(super::activity::now::better::migration::GettingBetter),
            Box::new(super::control_actions::guard::migration::Guard),
            Box::new(super::control_actions::guard::migration::GuardMeanings),
            Box::new(super::control_actions::guard::migration::GuardFacts),
            Box::new(super::control_actions::guard::migration::GuardProposals),
            Box::new(super::worker_control::model_usage::ModelUsage),
            Box::new(super::library::folders::LibraryFolders),
            Box::new(super::library::passages::LibraryPassages),
            Box::new(super::library::fence::LibraryFence),
            Box::new(super::library::meaning::LibraryMeaning),
            Box::new(super::library::always::LibraryAlways),
            Box::new(super::library::edits::LibraryEdits),
            Box::new(super::company_apps::CompanyApps),
            Box::new(super::library::tables::LibraryTables),
            Box::new(super::library::upkeep::LibraryUpkeep),
            Box::new(super::built_apps::BuiltApps),
            Box::new(super::built_apps::BuiltAppData),
            Box::new(super::built_apps::BuiltAppAgents),
            Box::new(super::built_apps::BuiltAppActions),
            Box::new(super::library::keeping::LibraryKeeping),
            Box::new(super::library::watching::LibraryWatching),
            Box::new(super::built_apps::BuiltAppEvents),
            Box::new(super::built_apps::BuiltAppAsking),
            Box::new(super::built_apps::BuiltAppChanges),
            Box::new(super::worker_control::model_usage::ModelRoutes),
            Box::new(super::worker_control::model_usage::ModelCosts),
            Box::new(super::model_budgets::ModelBudgets),
            Box::new(super::model_operations::ModelOperations),
            Box::new(super::operator::OperatorConsole),
            Box::new(super::message_search::MessageSearch),
            Box::new(super::invitation_delivery::InvitationDelivery),
            Box::new(super::unread::UnreadFromJoining),
            Box::new(super::transcripts::Transcripts),
            Box::new(super::chat::activity::ChatActivity),
            Box::new(super::work_signals::WorkNotifications),
            Box::new(super::chat::presence::MemberPresence),
            Box::new(super::chat_intake::several_agents::SeveralAgentsPerMessage),
            Box::new(super::handoff_migration::VisibleHandoffs),
            Box::new(super::message_actions::agent_reactions::AgentReactions),
            Box::new(super::web_reads::WebReads),
            Box::new(super::computers::ComputersMigration),
            Box::new(super::computer_browser_migration::ComputerBrowserMigration),
            Box::new(super::computer_policy::ComputerPolicyMigration),
            Box::new(super::computer_turns::ComputerTurnsMigration),
            Box::new(super::control_actions::outcomes::migration::UnconfirmedWrites),
            Box::new(super::connector_routing::ConnectorRouting),
            Box::new(super::event_subscriptions::ProviderEvents),
            Box::new(super::connector_calls::ConnectorOperations),
            Box::new(super::communications::migration::WhatsappNumbers),
            Box::new(super::communications::migration::DoorRooms),
            Box::new(super::communications::migration::AgentEmail),
        ]
    }
}
struct GuidedPermissions;
impl MigrationName for GuidedPermissions {
    fn name(&self) -> &str {
        "m20261025_000020_guided_permissions"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for GuidedPermissions {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(include_str!("setup_drafts.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Permission review receipts require a forward migration".into(),
        ))
    }
}
struct ConnectionAuthority;
impl MigrationName for ConnectionAuthority {
    fn name(&self) -> &str {
        "m20261025_000019_connection_authority"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for ConnectionAuthority {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(include_str!("connection_authority.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Connection authority requires a forward migration".into(),
        ))
    }
}
struct SharedControl;
impl MigrationName for SharedControl {
    fn name(&self) -> &str {
        "m20261025_000018_shared_control"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for SharedControl {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(include_str!("control.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Action receipts require a forward migration".into(),
        ))
    }
}
struct CloudExecutors;
impl MigrationName for CloudExecutors {
    fn name(&self) -> &str {
        "m20261024_000017_cloud_executors"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for CloudExecutors {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(include_str!("cloud_executors.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Cloud ownership requires a forward migration".into(),
        ))
    }
}
struct InteractiveReserve;
impl MigrationName for InteractiveReserve {
    fn name(&self) -> &str {
        "m20261024_000016_interactive_reserve"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for InteractiveReserve {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(include_str!("interactive_reserve.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Request classes require a forward migration".into(),
        ))
    }
}
struct HarnessPreparation;
impl MigrationName for HarnessPreparation {
    fn name(&self) -> &str {
        "m20261023_000015_harness_preparation"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for HarnessPreparation {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(include_str!("harness_preparation.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Preparation readiness requires a forward migration".into(),
        ))
    }
}
struct ReplyOutput;
impl MigrationName for ReplyOutput {
    fn name(&self) -> &str {
        "m20261023_000014_reply_output"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for ReplyOutput {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(include_str!("reply_output.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Reply output is durable; use a forward migration".into(),
        ))
    }
}
struct ExecutorSlots;
impl MigrationName for ExecutorSlots {
    fn name(&self) -> &str {
        "m20261023_000013_executor_slots"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for ExecutorSlots {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(include_str!("executor_slots.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Executor history requires a forward migration".into(),
        ))
    }
}
struct WorkerControl;
impl MigrationName for WorkerControl {
    fn name(&self) -> &str {
        "m20261023_000012_worker_control"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for WorkerControl {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(include_str!("worker_control.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Machine assignment receipts are durable; use a forward migration".into(),
        ))
    }
}
struct WorkerAssignments;
impl MigrationName for WorkerAssignments {
    fn name(&self) -> &str {
        "m20261023_000011_worker_assignments"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for WorkerAssignments {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(include_str!("worker_assignments.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Assignment history is durable; use a forward migration".into(),
        ))
    }
}
struct LoginFlows;
impl MigrationName for LoginFlows {
    fn name(&self) -> &str {
        "m20261023_000010_login_flows"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for LoginFlows {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared(
            "CREATE TABLE relay_login_flows(hash TEXT PRIMARY KEY CHECK(length(hash)=43), state TEXT NOT NULL CHECK(length(state)<=256), verifier TEXT NOT NULL CHECK(length(verifier)<=256), invitation TEXT CHECK(length(invitation)<=1024), expires BIGINT NOT NULL); CREATE INDEX relay_login_flows_expiry ON relay_login_flows(expires);"
        ).await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom("Use a forward migration".into()))
    }
}
struct Library;
impl MigrationName for Library {
    fn name(&self) -> &str {
        "m20261022_000009_library"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for Library {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(include_str!("library.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Library data is durable; use a forward migration".into(),
        ))
    }
}
struct Foundation;
impl MigrationName for Foundation {
    fn name(&self) -> &str {
        "m20261020_000001_workspace_foundation"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for Foundation {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(include_str!("schema.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Workspace data is durable; use a verified backup or a forward migration".into(),
        ))
    }
}
impl super::PgStore {
    pub async fn migrate(&self) -> anyhow::Result<()> {
        Migrator::up(&self.db, None).await?;
        Ok(())
    }
}

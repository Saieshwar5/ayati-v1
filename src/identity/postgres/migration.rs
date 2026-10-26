use sea_orm_migration::prelude::*;

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
            Box::new(ChannelApprovals),
            Box::new(super::durable_wait_migration::DurableWaits),
            Box::new(super::question_migration::Questions),
        ]
    }
}
struct ChannelApprovals;
impl MigrationName for ChannelApprovals {
    fn name(&self) -> &str {
        "m20261025_000021_channel_approvals"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for ChannelApprovals {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(include_str!("channel_approvals.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Approval delivery requires a forward migration".into(),
        ))
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

struct ConversationKinds;
impl MigrationName for ConversationKinds {
    fn name(&self) -> &str {
        "m20261020_000002_conversation_kinds"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for ConversationKinds {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager.get_connection().execute_unprepared("ALTER TABLE relay_chats ADD COLUMN kind TEXT NOT NULL DEFAULT 'group' CHECK(kind IN ('group','channel','direct')), ADD COLUMN description TEXT NOT NULL DEFAULT '', ADD COLUMN direct_key TEXT; ALTER TABLE relay_chat_messages ADD COLUMN thread TEXT; CREATE INDEX relay_messages_by_thread ON relay_chat_messages(workspace,chat,thread); CREATE UNIQUE INDEX relay_chat_direct_pair ON relay_chats(workspace,direct_key);").await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Conversation metadata is durable; use a forward migration".into(),
        ))
    }
}

struct DisplayNames;
impl MigrationName for DisplayNames {
    fn name(&self) -> &str {
        "m20261021_000003_display_names"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for DisplayNames {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE relay_users ADD COLUMN display_name TEXT")
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Display names are durable; use a forward migration".into(),
        ))
    }
}

struct Collaboration;
impl MigrationName for Collaboration {
    fn name(&self) -> &str {
        "m20261021_000004_company_collaboration"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for Collaboration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(include_str!("collaboration.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Company records are durable; use a forward migration".into(),
        ))
    }
}

struct QueueTelemetry;
impl MigrationName for QueueTelemetry {
    fn name(&self) -> &str {
        "m20261021_000005_queue_telemetry"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for QueueTelemetry {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(include_str!("telemetry.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom("Use a forward migration".into()))
    }
}

struct Messaging;
impl MigrationName for Messaging {
    fn name(&self) -> &str {
        "m20261021_000006_reliable_messaging"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for Messaging {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(include_str!("messaging.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Messages and receipts are durable; use a forward migration".into(),
        ))
    }
}

struct AgentProfiles;
impl MigrationName for AgentProfiles {
    fn name(&self) -> &str {
        "m20261021_000007_agent_profiles"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for AgentProfiles {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(include_str!("agent_profiles.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Agent ownership and knowledge are durable; use a forward migration".into(),
        ))
    }
}

struct WorkspaceRoles;
impl MigrationName for WorkspaceRoles {
    fn name(&self) -> &str {
        "m20261022_000008_workspace_roles"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for WorkspaceRoles {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(include_str!("workspace_roles.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
        Err(DbErr::Custom(
            "Ownership changes are durable; use a forward migration".into(),
        ))
    }
}

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
        ]
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

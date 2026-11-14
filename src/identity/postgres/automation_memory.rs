//! Automation memory (schema 115, E2 of how work reaches agents). Every
//! automation runs in a private work room and says what it remembers:
//! `fresh` opens a new room for every run, `remember` keeps one room, so
//! later runs see the earlier ones. Work never reads a conversation people use.
use super::*;
pub(crate) use migration::AutomationMemory;

/// What an automation remembers between runs.
pub(crate) const MEMORY: [&str; 2] = ["fresh", "remember"];

/// Whether this schema has automation memory (115+).
pub(super) async fn ready(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid=to_regclass('relay_event_triggers') AND attname='memory' AND NOT attisdropped) AS ready",
        vec![],
    )
    .await?
    .context("Missing schema check")?
    .try_get("", "ready")?)
}

/// The work room for one run of an event trigger: a new room for each run when
/// it starts fresh, else the trigger's own room, opened at its first run.
pub(super) async fn event_room(
    tx: &DatabaseTransaction,
    w: &str,
    trigger: &Value,
) -> Result<String> {
    let remember = trigger["memory"] == "remember";
    if remember && let Some(room) = trigger["room"].as_str() {
        return Ok(room.to_owned());
    }
    let id = trigger["id"].as_str().context("Missing trigger")?;
    let name = trigger["input"]["name"].as_str().unwrap_or("Event");
    let room = super::work_rooms::open(
        tx,
        w,
        name,
        "Runs of an event trigger",
        trigger["principal"].as_str().context("Missing person")?,
        trigger["agent"].as_str().context("Missing agent")?,
    )
    .await?;
    if remember {
        execute(
            tx,
            "UPDATE relay_event_triggers SET room=$3 WHERE workspace=$1 AND id=$2 AND room IS NULL",
            vec![w.into(), id.into(), room.clone().into()],
        )
        .await?;
    }
    Ok(room)
}

/// What Activity shows about a run that came from an event trigger: the same
/// shape as a schedule's `run_source`, plus the app and what it remembers.
pub(super) async fn run_source(
    db: &impl ConnectionTrait,
    w: &str,
    reply: &str,
) -> Result<Option<Value>> {
    if !ready(db).await? {
        return Ok(None);
    }
    Ok(json_rows(db, "SELECT jsonb_build_object('id',t.id,'name',t.input->>'name','agent',t.agent,'state',t.state,'revision',t.revision,
            'principal',t.principal,'memory',t.memory,'source',t.source_definition->>'name','post_to',c.name,
            'posted',p.state,'post_reason',p.reason,'room',d.room IS NOT NULL) AS data
        FROM relay_event_deliveries d JOIN relay_event_triggers t ON t.workspace=d.workspace AND t.id=d.trigger_id
        LEFT JOIN relay_chats c ON c.workspace=t.workspace AND c.id=t.chat
        LEFT JOIN relay_run_posts p ON p.workspace=d.workspace AND p.reply=d.reply
        WHERE d.workspace=$1 AND d.reply=$2", vec![w.into(), reply.into()]).await?.into_iter().next())
}

mod migration {
    use sea_orm_migration::prelude::*;
    pub(crate) struct AutomationMemory;
    impl MigrationName for AutomationMemory {
        fn name(&self) -> &str {
            "m20261114_000115_automation_memory"
        }
    }
    #[async_trait::async_trait]
    impl MigrationTrait for AutomationMemory {
        async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
            m.get_connection()
                .execute_unprepared(include_str!("automation_memory.sql"))
                .await?;
            Ok(())
        }
        async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
            Err(DbErr::Custom(
                "Automation memory requires a forward migration".into(),
            ))
        }
    }
}

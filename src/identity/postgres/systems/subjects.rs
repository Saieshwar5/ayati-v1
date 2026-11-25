//! Subjects (schema 118, C3 of the connected systems plan, D249). An event's
//! `subject` names one user of the outside system. For tools marked "Needs the
//! subject" Cuckoo fills the `subject` input from the event and hides it from the
//! agent, so it can't ask for another subject's data. Schema 151 removed the
//! per-subject rooms ("Per subject" memory) and per-subject parallel runs: a
//! trigger runs in its conversation, one delivery at a time.
use crate::identity::chat::model::Job;
use crate::identity::control::model::ActionRequest;
use crate::identity::postgres::*;
use serde_json::json;

pub(crate) async fn ready(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT EXISTS(SELECT 1 FROM pg_attribute WHERE attrelid=to_regclass('relay_event_deliveries') AND attname='subject' AND NOT attisdropped) AS ready",
        vec![],
    )
    .await?
    .context("Missing schema check")?
    .try_get("", "ready")?)
}

/// Whether `operation` of `connection` is a connected system's tool marked
/// "Needs the subject".
async fn marked(
    db: &impl ConnectionTrait,
    w: &str,
    connection: &str,
    operation: &str,
) -> Result<bool> {
    if !super::ready(db).await? {
        return Ok(false);
    }
    Ok(query(db, "SELECT 1 AS found FROM relay_connected_systems s, jsonb_array_elements(s.tools) t
        WHERE s.workspace=$1 AND s.connection=$2 AND NOT s.removed AND t->>'name'=$3 AND (t->>'subject')::boolean",
        vec![w.into(), connection.into(), operation.into()]).await?.is_some())
}

/// The subject of the event this run came from, if any.
async fn run_subject(db: &impl ConnectionTrait, job: &Job) -> Result<Option<String>> {
    if !ready(db).await? {
        return Ok(None);
    }
    query(db, "SELECT subject FROM relay_event_deliveries WHERE workspace=$1 AND reply=$2 AND NOT revoked",
        vec![job.workspace.clone().into(), job.reply.clone().into()]).await?
        .map(|r| r.try_get::<Option<String>>("", "subject"))
        .transpose()
        .map(Option::flatten)
        .map_err(Into::into)
}

/// Hides a marked tool's `subject` input from the agent.
pub(crate) async fn hide(db: &impl ConnectionTrait, w: &str, op: &mut Value) -> Result<()> {
    let (connection, operation) = (
        op["connection"].as_str().unwrap_or_default().to_owned(),
        op["operation"].as_str().unwrap_or_default().to_owned(),
    );
    if !marked(db, w, &connection, &operation).await? {
        return Ok(());
    }
    if let Some(properties) = op["schema"]["properties"].as_object_mut() {
        properties.remove("subject");
    }
    if let Some(required) = op["schema"]["required"].as_array_mut() {
        required.retain(|r| r != "subject");
    }
    op["subject"] = json!("Cuckoo fills in the subject of the event you are handling.");
    Ok(())
}

/// Fills a marked tool's `subject` from the run's event. The agent never
/// supplies it, and a run without a subject (a request in Chat, say) can't
/// use the tool at all.
pub(crate) async fn fill(
    db: &impl ConnectionTrait,
    job: &Job,
    request: &mut ActionRequest,
) -> Result<()> {
    if !marked(db, &job.workspace, &request.connection, &request.operation).await? {
        return Ok(());
    }
    ensure!(
        request.arguments.get("subject").is_none(),
        "Leave out subject: Cuckoo fills it in from the event"
    );
    let subject = run_subject(db, job)
        .await?
        .context("This tool works only in a run started by an event about one subject")?;
    request
        .arguments
        .as_object_mut()
        .context("Expected operation fields")?
        .insert("subject".into(), json!(subject));
    Ok(())
}

impl PgStore {
    /// Fills a marked tool's subject before the action is proposed.
    pub(crate) async fn fill_subject(&self, job: &Job, request: &mut ActionRequest) -> Result<()> {
        fill(&self.db, job, request).await
    }
}

pub(crate) use migration::Subjects;
mod migration {
    use sea_orm_migration::prelude::*;
    pub(crate) struct Subjects;
    impl MigrationName for Subjects {
        fn name(&self) -> &str {
            "m20261114_000118_subjects"
        }
    }
    #[async_trait::async_trait]
    impl MigrationTrait for Subjects {
        async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
            m.get_connection()
                .execute_unprepared(include_str!("subjects.sql"))
                .await?;
            Ok(())
        }
        async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
            Err(DbErr::Custom("Subjects require a forward migration".into()))
        }
    }
}

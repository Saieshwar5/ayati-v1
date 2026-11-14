//! Subjects (schema 118, C3 of the connected systems plan, D249). An event's
//! `subject` names one user of the outside system. Cuckoo does three things
//! with it: a private room per agent, system and subject ("Per subject"
//! memory); one unfinished run per automation and subject, so different
//! subjects run side by side and one subject's events stay in order; and for
//! tools marked "Needs the subject" Cuckoo fills the `subject` input from the
//! event and hides it from the agent, so it can't ask for another subject's data.
use crate::identity::chat::model::Job;
use crate::identity::control::model::ActionRequest;
use crate::identity::postgres::*;
use serde_json::json;

pub(crate) async fn ready(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regclass('relay_subject_rooms') IS NOT NULL AS ready",
        vec![],
    )
    .await?
    .context("Missing schema check")?
    .try_get("", "ready")?)
}

/// The room for a "Per subject" run: the agent's room for this system and
/// subject, opened at its first run. `None` when the event named no subject.
pub(crate) async fn room(
    tx: &DatabaseTransaction,
    w: &str,
    trigger: &Value,
    subject: Option<&str>,
) -> Result<Option<String>> {
    let Some(subject) = subject else {
        return Ok(None);
    };
    let agent = trigger["agent"].as_str().context("Missing agent")?;
    let connection = trigger["connection"].as_str().context("Missing system")?;
    if let Some(row) = query(tx, "SELECT room FROM relay_subject_rooms WHERE workspace=$1 AND agent=$2 AND connection=$3 AND subject=$4",
        vec![w.into(), agent.into(), connection.into(), subject.into()]).await?
    {
        return Ok(Some(row.try_get("", "room")?));
    }
    let name: String = format!(
        "{} · {subject}",
        trigger["input"]["name"].as_str().unwrap_or("Event")
    )
    .chars()
    .take(160)
    .collect();
    let room = super::super::work_rooms::open(
        tx,
        w,
        &name,
        "Runs about one subject of a connected system",
        trigger["principal"].as_str().context("Missing person")?,
        agent,
    )
    .await?;
    execute(tx, "INSERT INTO relay_subject_rooms(workspace,agent,connection,subject,room) VALUES($1,$2,$3,$4,$5)",
        vec![w.into(), agent.into(), connection.into(), subject.into(), room.clone().into()]).await?;
    Ok(Some(room))
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

    /// "Forget this subject": its rooms with this system's agents stop being
    /// remembered. Their messages are cleared and the next event starts fresh.
    pub(crate) async fn forget_subject(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        system: &str,
        subject: &str,
    ) -> Result<Value> {
        ensure!(
            !subject.is_empty() && subject.len() <= 128 && !subject.chars().any(char::is_control),
            "Name the subject to forget"
        );
        let tx = self.workspace_tx(w).await?;
        super::connect::may_change(&tx, user, w, agent).await?;
        super::connect::saved(&tx, w, agent, system).await?;
        let rooms = rows(&tx, "DELETE FROM relay_subject_rooms WHERE workspace=$1 AND connection=$2 AND subject=$3 RETURNING room",
            vec![w.into(), system.into(), subject.into()]).await?;
        for row in &rooms {
            let room: String = row.try_get("", "room")?;
            execute(&tx, "UPDATE relay_chat_messages SET content='Forgotten at the request of the system it came from.',attachments='[]'::jsonb WHERE workspace=$1 AND chat=$2",
                vec![w.into(), room.clone().into()]).await?;
            execute(
                &tx,
                "DELETE FROM relay_working_context WHERE workspace=$1 AND chat=$2",
                vec![w.into(), room.into()],
            )
            .await?;
        }
        audit(
            &tx,
            &user.id,
            Some(w),
            "connected_system_subject_forgotten",
            &format!("{agent}/{system}"),
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"forgotten":rooms.len()}))
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

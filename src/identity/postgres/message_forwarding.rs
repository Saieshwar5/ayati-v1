//! Forward a message to a person or an agent (schema 125).
//!
//! A forward is a new message in your direct chat with them: your own words,
//! with a copy of the message you forward (who wrote it, where, when and what
//! it said), kept as it was when you forwarded it.
//!
//! - You must be able to read the original, and to talk to the agent you send
//!   it to. Sending it to an agent asks that agent, with your words as the
//!   request; the copy is information someone else wrote, never an instruction.
//! - Answers built from restricted sources (Library documents, connected-event
//!   data, hand-offs that used them) and messages in a connected system's
//!   subject room can't be forwarded: a copy would escape the checks and the
//!   erasure that protect them.
//! - Deleting the forward removes the copy; editing or deleting the original
//!   leaves it as it was.
//! - The message's own files travel when asked for: each is copied into the
//!   direct chat, as an attachment of the forward that the copy names. A file
//!   belongs to one conversation, so the copy is a file of its own.
//! - Your own documents are ordinary attachments of the forward.
use super::*;
use crate::identity::chat::model::{Forward, Job, NewChat, NewMessage};
use crate::identity::collaboration::Object;
use serde_json::json;

#[cfg(test)]
#[path = "message_forwarding_files_tests.rs"]
mod file_tests;
#[cfg(test)]
#[path = "message_forwarding_tests.rs"]
mod tests;

/// Stored as the content of a forward sent to a person without a message of its own.
pub(crate) const NO_NOTE: &str = "Forwarded message";

/// Whether schema 125 is applied. Upgrade tests run this code on older schemas.
async fn ready(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT EXISTS(SELECT 1 FROM information_schema.columns WHERE table_schema=current_schema()
           AND table_name='relay_chat_messages' AND column_name='forwarded') AS ready",
        vec![],
    )
    .await?
    .context("Missing schema check")?
    .try_get("", "ready")?)
}

/// Where the forwarded message was, as its reader is told.
fn place(kind: &str, name: &str) -> String {
    match kind {
        "direct" => "Direct message".into(),
        "group" => "Group".into(),
        _ => format!("#{name}"),
    }
}

/// The ID a carried file's copy gets. The same forward, retried, names the
/// same copies, so it stores and sends them once.
pub(crate) fn copy_id(forward: &str, file: &str) -> String {
    let digest = ring::digest::digest(
        &ring::digest::SHA256,
        format!("forward:{forward}:{file}").as_bytes(),
    );
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest.as_ref()[..16]);
    uuid::Builder::from_custom_bytes(bytes)
        .into_uuid()
        .to_string()
}

/// The copy of `message` a forward carries, when this person may forward it,
/// and the IDs of the files attached to that message.
async fn copy(
    tx: &DatabaseTransaction,
    w: &str,
    chat: &str,
    message: &str,
) -> Result<(Value, Vec<String>)> {
    let exclusions = super::conversation_context::source_exclusions(tx).await?;
    let subject_rooms = if query(
        tx,
        "SELECT to_regclass('relay_subject_rooms') IS NOT NULL AS present",
        vec![],
    )
    .await?
    .context("Missing schema check")?
    .try_get::<bool>("", "present")?
    {
        " AND NOT EXISTS(SELECT 1 FROM relay_subject_rooms s WHERE s.workspace=m.workspace AND s.room=m.chat)"
    } else {
        ""
    };
    let row = query(
        tx,
        &format!(
            "SELECT m.kind,m.content,m.at,m.attachments,c.kind AS chat_kind,c.name AS chat_name,to_jsonb(m)->'forwarded' AS forwarded,
                CASE WHEN m.kind='agent' THEN a.name ELSE u.display_name END AS name
             FROM relay_chat_messages m
             JOIN relay_chats c ON c.workspace=m.workspace AND c.id=m.chat
             LEFT JOIN relay_users u ON u.id=m.author AND m.kind='user'
             LEFT JOIN relay_shared_agents a ON a.workspace=m.workspace AND a.id=m.author AND m.kind='agent'
             WHERE m.workspace=$1 AND m.chat=$2 AND m.id=$3 AND m.state IN ('sent','complete')
               AND btrim(m.content)<>''{subject_rooms}{exclusions}"
        ),
        vec![w.into(), chat.into(), message.into()],
    )
    .await?
    .context("This message can't be forwarded")?;
    let content: String = row.try_get("", "content")?;
    let inner: Option<Value> = row.try_get("", "forwarded")?;
    let attached: Vec<String> = serde_json::from_value(row.try_get("", "attachments")?)?;
    // Forwarding a forward that had no words of its own passes on the original.
    // Its files are named again from the copies this forward makes.
    if let Some(mut inner) = inner.filter(|v| v.is_object())
        && content == NO_NOTE
    {
        if let Some(fields) = inner.as_object_mut() {
            fields.remove("files");
        }
        return Ok((inner, attached));
    }
    let kind: String = row.try_get("", "kind")?;
    let name: Option<String> = row.try_get("", "name")?;
    // A message that only shares files has a placeholder for content.
    let text = if content == super::transcripts::FILES_ONLY && !attached.is_empty() {
        String::new()
    } else {
        content
    };
    let copy = json!({
        "name": name.filter(|n| !n.trim().is_empty()).unwrap_or_else(|| {
            if kind == "agent" { "An agent" } else { "A workspace member" }.into()
        }),
        "kind": if kind == "agent" { "agent" } else { "user" },
        "place": place(&row.try_get::<String>("", "chat_kind")?, &row.try_get::<String>("", "chat_name")?),
        "at": row.try_get::<i64>("", "at")?,
        "text": text,
    });
    Ok((copy, attached))
}

/// What a forward may carry: only files of the forwarded message, each once,
/// eight with your own documents, and something to read.
fn check_files(input: &Forward, copy: &Value, attached: &[String]) -> Result<()> {
    ensure!(
        input.files.len() + input.attachments.len() <= 8,
        "A message can carry at most eight files"
    );
    for (index, file) in input.files.iter().enumerate() {
        ensure!(
            attached.contains(file) && !input.files[..index].contains(file),
            "That file is not part of the message you are forwarding"
        );
    }
    ensure!(
        !input.files.is_empty() || copy["text"].as_str().is_some_and(|text| !text.is_empty()),
        "Choose at least one of its files to forward"
    );
    Ok(())
}

/// Copies the forwarded message's chosen files into the chat the forward goes
/// to, as this person's files. Returns what the copy says about each. The
/// bytes are stored under the same IDs before this runs (collaboration/files.rs).
async fn carry(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    (from, to): (&str, &str),
    forward: &str,
    files: &[String],
) -> Result<Vec<Value>> {
    let mut carried = vec![];
    for file in files {
        let id = copy_id(forward, file);
        execute(
            tx,
            "INSERT INTO relay_objects(workspace,chat,id,uploader,name,mime,size,checksum,created)
             SELECT workspace,$3,$4,$5,name,mime,size,checksum,$6 FROM relay_objects
             WHERE workspace=$1 AND chat=$2 AND id=$7 ON CONFLICT (workspace,id) DO NOTHING",
            vec![
                w.into(),
                from.into(),
                to.into(),
                id.clone().into(),
                user.id.clone().into(),
                now().into(),
                file.clone().into(),
            ],
        )
        .await?;
        super::transcripts::copy_in(tx, w, file, &id).await?;
        let row = query(
            tx,
            "SELECT name,mime,size FROM relay_objects WHERE workspace=$1 AND chat=$2 AND id=$3 AND uploader=$4",
            vec![w.into(), to.into(), id.clone().into(), user.id.clone().into()],
        )
        .await?
        .context("That file is no longer available")?;
        carried.push(json!({
            "id": id,
            "name": row.try_get::<String>("", "name")?,
            "mime": row.try_get::<String>("", "mime")?,
            "size": row.try_get::<i64>("", "size")?,
        }));
    }
    Ok(carried)
}

impl PgStore {
    /// The files of `message` this person asks a forward to carry, as stored,
    /// when they may forward it. Their bytes are copied before the forward is saved.
    pub(crate) async fn forward_sources(
        &self,
        user: &User,
        w: &str,
        chat: &str,
        message: &str,
        input: &Forward,
    ) -> Result<Vec<Object>> {
        let tx = self.read_tx(w).await?;
        ensure!(ready(&tx).await?, "Forwarding is not available yet");
        super::chat::access(&tx, user, w, chat).await?;
        let (forwarded, attached) = copy(&tx, w, chat, message).await?;
        check_files(input, &forwarded, &attached)?;
        let mut sources = vec![];
        for file in &input.files {
            let row = query(
                &tx,
                "SELECT to_jsonb(o) AS data FROM relay_objects o WHERE workspace=$1 AND chat=$2 AND id=$3",
                vec![w.into(), chat.into(), file.clone().into()],
            )
            .await?
            .context("That file is no longer available")?;
            sources.push(serde_json::from_value(row.try_get("", "data")?)?);
        }
        tx.commit().await?;
        Ok(sources)
    }
    /// Forward `message` of `chat` to a person or an agent. Returns the direct
    /// chat it went to and the agent's job, when it went to an agent.
    pub async fn forward_message(
        &self,
        user: &User,
        w: &str,
        chat: &str,
        message: &str,
        input: &Forward,
    ) -> Result<(String, Option<Job>)> {
        ensure!(
            ["user", "agent"].contains(&input.to.kind.as_str()),
            "Forward to a person or an agent"
        );
        ensure!(input.note.len() <= 4000, "Your message is too long");
        let note = input.note.trim();
        let tx = self.workspace_tx(w).await?;
        ensure!(ready(&tx).await?, "Forwarding is not available yet");
        super::chat::access(&tx, user, w, chat).await?;
        let (mut forwarded, attached) = copy(&tx, w, chat, message).await?;
        check_files(input, &forwarded, &attached)?;
        let name = if input.to.kind == "agent" {
            let row = query(
                &tx,
                "SELECT name FROM relay_shared_agents WHERE workspace=$1 AND id=$2",
                vec![w.into(), input.to.id.clone().into()],
            )
            .await?
            .context("Agent is not shared with you")?;
            let name: String = row.try_get("", "name")?;
            // Your words are the request; the copy alone asks nothing.
            ensure!(!note.is_empty(), "Say what you need from {name}");
            name
        } else {
            ensure!(input.to.id != user.id, "Forward it to someone else");
            "Direct message".into()
        };
        let direct = NewChat {
            id: input.chat.clone(),
            name,
            users: if input.to.kind == "user" {
                vec![input.to.id.clone()]
            } else {
                vec![]
            },
            agents: if input.to.kind == "agent" {
                vec![input.to.id.clone()]
            } else {
                vec![]
            },
            kind: "direct".into(),
            description: String::new(),
        };
        let target = super::chat::chat_create_in(&tx, user, w, &direct).await?;
        let carried = carry(&tx, user, w, (chat, &target), &input.id, &input.files).await?;
        // The copies come first, then your own documents.
        let mut attachments: Vec<String> = carried
            .iter()
            .filter_map(|file| file["id"].as_str().map(str::to_owned))
            .collect();
        attachments.extend(input.attachments.iter().cloned());
        if !carried.is_empty() {
            forwarded["files"] = json!(carried);
        }
        let sent = NewMessage {
            id: input.id.clone(),
            content: if note.is_empty() {
                NO_NOTE.into()
            } else {
                note.into()
            },
            agent: None,
            agents: if input.to.kind == "agent" {
                vec![input.to.id.clone()]
            } else {
                vec![]
            },
            thread: None,
            reply_to: None,
            attachments,
        };
        let jobs = self
            .forward_in(&tx, user, w, &target, &sent, &forwarded)
            .await?;
        tx.commit().await?;
        Ok((target, jobs.into_iter().next()))
    }
}

/// Deleting a message removes the copy it forwarded too.
pub(in crate::identity::postgres) async fn erase(
    tx: &DatabaseTransaction,
    w: &str,
    message: &str,
) -> Result<()> {
    if ready(tx).await? {
        execute(
            tx,
            "UPDATE relay_chat_messages SET forwarded=NULL WHERE workspace=$1 AND id=$2",
            vec![w.into(), message.into()],
        )
        .await?;
    }
    Ok(())
}

pub(crate) use migration::{ForwardingAndGuests, GuestsRemoved};
mod migration {
    use sea_orm_migration::prelude::*;
    pub(crate) struct ForwardingAndGuests;
    impl MigrationName for ForwardingAndGuests {
        fn name(&self) -> &str {
            "m20261116_000125_forwarding_and_guests"
        }
    }
    #[async_trait::async_trait]
    impl MigrationTrait for ForwardingAndGuests {
        async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
            m.get_connection()
                .execute_unprepared(include_str!("forwarding_and_guests.sql"))
                .await?;
            Ok(())
        }
        async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
            Err(DbErr::Custom(
                "Forwarding requires a forward migration".into(),
            ))
        }
    }
    /// Schema 130: agents invited into a thread of a private chat are gone;
    /// an assistant's threads (schema 128) do that job.
    pub(crate) struct GuestsRemoved;
    impl MigrationName for GuestsRemoved {
        fn name(&self) -> &str {
            "m20261117_000130_thread_guests_removed"
        }
    }
    #[async_trait::async_trait]
    impl MigrationTrait for GuestsRemoved {
        async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
            m.get_connection()
                .execute_unprepared("DROP TABLE relay_thread_guests;")
                .await?;
            Ok(())
        }
        async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
            Err(DbErr::Custom(
                "Removing thread guests requires a forward migration".into(),
            ))
        }
    }
}

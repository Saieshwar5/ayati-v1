//! What people said in voice notes and other audio attachments (schema 95).
//!
//! An audio upload queues one row here when the controller has a transcription
//! model. Controllers take rows with a lease, call the provider and save the text
//! or the failure; the row belongs to its object and is kept with it. People get
//! the transcript with the message's attachments, and agents get it in the
//! conversation evidence, so both follow the message's own access rules.
use super::*;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use serde_json::json;

#[cfg(test)]
#[path = "transcripts_tests.rs"]
mod tests;

/// Tries per transcript; a lease that runs out counts as a try.
pub(crate) const ATTEMPTS: i16 = 3;
/// How long one controller holds a transcript: reading the bytes and the
/// provider's time limit, with room to save the result.
pub(crate) const LEASE_MS: i64 = 150_000;
/// Longest transcript kept; five minutes of speech is about 5,000 characters.
pub(crate) const MAX_TEXT: usize = 16_000;
/// Most of one transcript an agent reads, so older messages still fit.
const AGENT_TEXT: usize = 8_000;
/// What the web client sends for a message that only shares files.
const FILES_ONLY: &str = "Shared files";

pub(crate) struct Transcripts;
impl MigrationName for Transcripts {
    fn name(&self) -> &str {
        "m20261107_000095_voice_transcripts"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for Transcripts {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        // One row per audio object, updated in place: not an append-only history.
        // The pending index is the queue (a deliberate scan across workspaces,
        // like the dispatcher's); everything people read goes by workspace first.
        m.get_connection()
            .execute_unprepared(
                "CREATE TABLE relay_object_transcripts (
                  workspace TEXT NOT NULL, object TEXT NOT NULL,
                  status TEXT NOT NULL DEFAULT 'pending' CHECK(status IN ('pending','ready','failed')),
                  text TEXT CHECK(char_length(text)<=16000),
                  duration_ms INTEGER CHECK(duration_ms>=0),
                  model TEXT CHECK(char_length(model)<=200),
                  attempts SMALLINT NOT NULL DEFAULT 0 CHECK(attempts BETWEEN 0 AND 3),
                  claim TEXT, lease_until_ms BIGINT NOT NULL DEFAULT 0,
                  created_ms BIGINT NOT NULL DEFAULT (extract(epoch FROM clock_timestamp())*1000)::bigint,
                  finished_ms BIGINT,
                  PRIMARY KEY(workspace,object),
                  FOREIGN KEY(workspace,object) REFERENCES relay_objects(workspace,id),
                  CHECK((status='ready')=(text IS NOT NULL)));
                 CREATE INDEX relay_object_transcripts_pending ON relay_object_transcripts(created_ms)
                  WHERE status='pending';",
            )
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Transcripts require a forward migration".into(),
        ))
    }
}

/// A transcript this controller holds until `LEASE_MS` passes.
#[derive(Clone)]
pub(crate) struct Claimed {
    pub workspace: String,
    pub object: String,
    pub claim: String,
    pub name: String,
    pub mime: String,
    pub size: i64,
    pub checksum: String,
    pub uploader: String,
}

pub(crate) enum Outcome {
    Heard {
        text: String,
        duration_ms: Option<i32>,
        model: String,
    },
    /// Worth another try: the provider was busy, unreachable or too slow.
    Retry,
    /// Never worth another try: too long, unreadable or refused.
    Failed { duration_ms: Option<i32> },
}

const NOW_MS: &str = "(extract(epoch FROM clock_timestamp())*1000)::bigint";

/// Queues an uploaded audio object, in the transaction that saves it.
pub(super) async fn queue_in(tx: &DatabaseTransaction, w: &str, object: &str) -> Result<()> {
    execute(
        tx,
        "INSERT INTO relay_object_transcripts(workspace,object) VALUES($1,$2)",
        vec![w.into(), object.into()],
    )
    .await?;
    Ok(())
}

impl PgStore {
    /// Takes up to `limit` waiting transcripts. Two controllers never take the
    /// same one: rows are locked with SKIP LOCKED and leased in one statement.
    pub(crate) async fn claim_transcripts(&self, limit: i64) -> Result<Vec<Claimed>> {
        let tx = self.db.begin().await?;
        // The last try's lease ran out (its controller stopped): give up.
        let expired = rows(&tx, &format!("UPDATE relay_object_transcripts SET status='failed',claim=NULL,finished_ms={NOW_MS}
            WHERE status='pending' AND attempts>={ATTEMPTS} AND lease_until_ms<={NOW_MS} RETURNING workspace,object"), vec![]).await?;
        for row in expired {
            let (w, object): (String, String) =
                (row.try_get("", "workspace")?, row.try_get("", "object")?);
            announce(&tx, &w, &object).await?;
        }
        let claim = uuid::Uuid::new_v4().to_string();
        let taken = rows(&tx, &format!("WITH next AS (SELECT workspace,object FROM relay_object_transcripts
              WHERE status='pending' AND attempts<{ATTEMPTS} AND lease_until_ms<={NOW_MS}
              ORDER BY created_ms LIMIT $2 FOR UPDATE SKIP LOCKED)
            UPDATE relay_object_transcripts t SET claim=$1,attempts=t.attempts+1,lease_until_ms={NOW_MS}+{LEASE_MS}
            FROM next, relay_objects o WHERE t.workspace=next.workspace AND t.object=next.object
              AND o.workspace=t.workspace AND o.id=t.object
            RETURNING t.workspace,t.object,o.name,o.mime,o.size,o.checksum,o.uploader"),
            vec![claim.clone().into(), limit.into()]).await?;
        tx.commit().await?;
        taken
            .into_iter()
            .map(|row| {
                Ok(Claimed {
                    workspace: row.try_get("", "workspace")?,
                    object: row.try_get("", "object")?,
                    claim: claim.clone(),
                    name: row.try_get("", "name")?,
                    mime: row.try_get("", "mime")?,
                    size: row.try_get("", "size")?,
                    checksum: row.try_get("", "checksum")?,
                    uploader: row.try_get("", "uploader")?,
                })
            })
            .collect()
    }

    /// How long until a waiting transcript's retry pause or lease ends, when it
    /// can be taken (or given up) again; None when none is waiting for a time.
    pub(crate) async fn next_transcript_in(&self) -> Result<Option<std::time::Duration>> {
        let wait = query(
            &self.db,
            &format!(
                "SELECT min(lease_until_ms)-{NOW_MS} AS wait FROM relay_object_transcripts
            WHERE status='pending' AND lease_until_ms>{NOW_MS}"
            ),
            vec![],
        )
        .await?
        .context("Transcript queue unavailable")?
        .try_get::<Option<i64>>("", "wait")?;
        Ok(wait.map(|ms| std::time::Duration::from_millis(ms.max(0) as u64)))
    }

    /// Saves what happened to a held transcript. False when the lease was lost
    /// and another controller took it, so this result is dropped.
    pub(crate) async fn finish_transcript(&self, item: &Claimed, outcome: Outcome) -> Result<bool> {
        let tx = self.workspace_tx(&item.workspace).await?;
        let key: Vec<sea_orm::Value> = vec![
            item.workspace.clone().into(),
            item.object.clone().into(),
            item.claim.clone().into(),
        ];
        let held = "workspace=$1 AND object=$2 AND claim=$3 AND status='pending'";
        let changed = match outcome {
            Outcome::Heard {
                text,
                duration_ms,
                model,
            } => {
                let text: String = text.trim().chars().take(MAX_TEXT).collect();
                let mut args = key;
                args.extend([text.into(), duration_ms.into(), model.into()]);
                execute(&tx, &format!("UPDATE relay_object_transcripts SET status='ready',text=$4,duration_ms=$5,model=$6,claim=NULL,finished_ms={NOW_MS} WHERE {held}"), args).await?
            }
            Outcome::Failed { duration_ms } => {
                let mut args = key;
                args.push(duration_ms.into());
                execute(&tx, &format!("UPDATE relay_object_transcripts SET status='failed',duration_ms=$4,claim=NULL,finished_ms={NOW_MS} WHERE {held}"), args).await?
            }
            // Wait a little longer after each try; the last try fails for good.
            Outcome::Retry => {
                execute(
                    &tx,
                    &format!(
                        "UPDATE relay_object_transcripts SET claim=NULL,
                    status=CASE WHEN attempts>={ATTEMPTS} THEN 'failed' ELSE 'pending' END,
                    finished_ms=CASE WHEN attempts>={ATTEMPTS} THEN {NOW_MS} END,
                    lease_until_ms={NOW_MS}+10000*attempts WHERE {held}"
                    ),
                    key,
                )
                .await?
            }
        };
        if changed == 1 {
            let finished = query(&tx, "SELECT status<>'pending' AS finished FROM relay_object_transcripts WHERE workspace=$1 AND object=$2", vec![item.workspace.clone().into(), item.object.clone().into()])
                .await?.context("Transcript missing")?.try_get::<bool>("", "finished")?;
            if finished {
                announce(&tx, &item.workspace, &item.object).await?;
            }
        }
        tx.commit().await?;
        Ok(changed == 1)
    }
}

/// Tells open conversations that a message's attachment changed, through the
/// existing change stream; each reader's access is checked when it reloads.
pub(super) async fn announce(tx: &DatabaseTransaction, w: &str, object: &str) -> Result<()> {
    let messages = rows(tx, "SELECT m.id,m.chat FROM relay_objects o JOIN relay_chat_messages m ON m.workspace=o.workspace AND m.chat=o.chat
        WHERE o.workspace=$1 AND o.id=$2 AND m.attachments @> jsonb_build_array(o.id) LIMIT 8", vec![w.into(), object.into()]).await?;
    for message in &messages {
        let cursor: i64 = query(
            tx,
            "INSERT INTO relay_sync_clocks(workspace,cursor) VALUES($1,1)
             ON CONFLICT(workspace) DO UPDATE SET cursor=relay_sync_clocks.cursor+1 RETURNING cursor",
            vec![w.into()],
        )
        .await?
        .context("Change clock unavailable")?
        .try_get("", "cursor")?;
        execute(tx, "INSERT INTO relay_sync_events(workspace,cursor,chat,message,kind) VALUES($1,$2,$3,$4,'message')",
            vec![w.into(), cursor.into(), message.try_get::<String>("", "chat")?.into(), message.try_get::<String>("", "id")?.into()]).await?;
    }
    if !messages.is_empty() {
        query(
            tx,
            "SELECT pg_notify('relay_message_changes',$1)::text AS sent",
            vec![w.into()],
        )
        .await?;
    }
    Ok(())
}

/// The saved transcripts of these objects: status, text and duration.
async fn saved(
    tx: &DatabaseTransaction,
    w: &str,
    objects: &[String],
) -> Result<std::collections::HashMap<String, (String, Option<String>, Option<i32>)>> {
    let mut found = std::collections::HashMap::new();
    if objects.is_empty() {
        return Ok(found);
    }
    for row in rows(tx, "SELECT object,status,text,duration_ms FROM relay_object_transcripts WHERE workspace=$1 AND object IN (SELECT jsonb_array_elements_text($2::jsonb))", vec![w.into(), json!(objects).into()]).await? {
        found.insert(
            row.try_get("", "object")?,
            (row.try_get("", "status")?, row.try_get("", "text")?, row.try_get("", "duration_ms")?),
        );
    }
    Ok(found)
}

fn audio(mime: &str) -> bool {
    mime.starts_with("audio/")
}

/// Adds `transcript_status` (pending, ready, failed or not_enabled), `transcript`
/// and `duration_ms` to the audio attachments of messages a person may read.
pub(super) async fn for_people(
    tx: &DatabaseTransaction,
    w: &str,
    messages: &mut [Value],
) -> Result<()> {
    let objects: Vec<String> = messages
        .iter()
        .flat_map(|m| m["attachments"].as_array().cloned().unwrap_or_default())
        .filter(|file| audio(file["mime"].as_str().unwrap_or("")))
        .filter_map(|file| file["id"].as_str().map(str::to_owned))
        .collect();
    let found = saved(tx, w, &objects).await?;
    for message in messages.iter_mut() {
        let Some(files) = message["attachments"].as_array_mut() else {
            continue;
        };
        for file in files
            .iter_mut()
            .filter(|file| audio(file["mime"].as_str().unwrap_or("")))
        {
            let saved = file["id"].as_str().and_then(|id| found.get(id));
            file["transcript_status"] = json!(saved.map_or("not_enabled", |s| s.0.as_str()));
            file["transcript"] = json!(saved.and_then(|s| s.1.clone()));
            file["duration_ms"] = json!(saved.and_then(|s| s.2));
        }
    }
    Ok(())
}

/// Replaces each message's attachment IDs with what an agent may know of them:
/// name, mime, size and, for audio, its transcript or why there is none.
pub(super) async fn for_agents(
    tx: &DatabaseTransaction,
    w: &str,
    chat: &str,
    messages: &mut [Value],
) -> Result<()> {
    let ids: Vec<String> = messages
        .iter()
        .flat_map(|m| m["attachments"].as_array().cloned().unwrap_or_default())
        .filter_map(|id| id.as_str().map(str::to_owned))
        .collect();
    let mut objects = std::collections::HashMap::new();
    if !ids.is_empty() {
        for row in rows(tx, "SELECT id,name,mime,size FROM relay_objects WHERE workspace=$1 AND chat=$2 AND id IN (SELECT jsonb_array_elements_text($3::jsonb))", vec![w.into(), chat.into(), json!(ids).into()]).await? {
            let id: String = row.try_get("", "id")?;
            objects.insert(id, (row.try_get::<String>("", "name")?, row.try_get::<String>("", "mime")?, row.try_get::<i64>("", "size")?));
        }
    }
    let found = saved(tx, w, &ids).await?;
    for message in messages.iter_mut() {
        let ids = message
            .as_object_mut()
            .and_then(|m| m.remove("attachments"))
            .and_then(|a| a.as_array().cloned())
            .unwrap_or_default();
        let files: Vec<Value> = ids
            .iter()
            .filter_map(|id| Some((id.as_str()?, objects.get(id.as_str()?)?)))
            .map(|(id, (name, mime, size))| {
                let mut file = json!({"name": name, "mime": mime, "size": size});
                if audio(mime) {
                    describe_audio(&mut file, found.get(id));
                }
                file
            })
            .collect();
        if files.is_empty() {
            continue;
        }
        // A message that only shares files has no words of its own.
        if message["content"] == FILES_ONLY {
            message["content"] = json!("");
        }
        message["attachments"] = json!(files);
    }
    Ok(())
}

fn describe_audio(file: &mut Value, saved: Option<&(String, Option<String>, Option<i32>)>) {
    file["transcript_status"] = json!(saved.map_or("not_enabled", |s| s.0.as_str()));
    if let Some(ms) = saved.and_then(|s| s.2) {
        file["duration_seconds"] = json!((i64::from(ms) + 500) / 1000);
    }
    if let Some(text) = saved.and_then(|s| s.1.as_deref()) {
        let kept: String = text.chars().take(AGENT_TEXT).collect();
        if kept.len() < text.len() {
            file["transcript_truncated"] = json!(true);
        }
        file["transcript"] = json!(kept);
    }
}

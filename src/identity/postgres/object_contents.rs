//! What agents can read in a chat file (schema 100).
//!
//! Every non-audio upload queues one row here in the transaction that saves the
//! file. Controllers take rows with a lease and run the Library's sandboxed local
//! readers over the stored bytes (`file_reading`): PDF, Word, Excel, plain text,
//! and text recognition for images where Tesseract is installed. Nothing leaves
//! the controller. The text is kept beside the file and goes with it, and every
//! way it reaches a person or an agent follows the message's own access rules.
use super::super::*;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use serde_json::json;
use std::collections::HashMap;

#[cfg(test)]
#[path = "object_contents_tests.rs"]
pub(in crate::identity::postgres) mod tests;

/// Tries per file; a lease that runs out counts as a try.
pub(crate) const ATTEMPTS: i16 = 3;
/// How long one controller holds a file: reading it from storage (30 s at most)
/// and the slowest reader (text recognition, 25 s), with room to save.
pub(crate) const LEASE_MS: i64 = 120_000;
/// Longest text kept, the Library's limit.
pub(crate) const MAX_TEXT: usize = 64_000;

pub(crate) struct FileReading;
impl MigrationName for FileReading {
    fn name(&self) -> &str {
        "m20261107_000100_chat_file_reading"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for FileReading {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("object_contents.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Chat file reading requires a forward migration".into(),
        ))
    }
}

/// A file this controller holds until `LEASE_MS` passes.
#[derive(Clone)]
pub(crate) struct Claimed {
    pub workspace: String,
    pub object: String,
    pub claim: String,
    pub name: String,
    pub mime: String,
    pub size: i64,
    pub checksum: String,
}

pub(crate) enum Outcome {
    /// Text was found. `truncated` when the file holds more than was kept.
    Read {
        kind: &'static str,
        text: String,
        truncated: bool,
    },
    /// Never worth another try: a format no reader handles, or no text in it.
    Unreadable { kind: Option<&'static str> },
    /// Worth another try: the stored bytes couldn't be fetched.
    Retry,
}

/// What was read from one file.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Reading {
    /// pending, ready, unsupported or failed.
    pub status: String,
    pub kind: Option<String>,
    pub text: Option<String>,
    pub truncated: bool,
}

const NOW_MS: &str = "(extract(epoch FROM clock_timestamp())*1000)::bigint";

/// Queues an uploaded file for reading, in the transaction that saves it.
/// Audio has its own transcript instead (schema 95).
pub(in crate::identity::postgres) async fn queue_in(
    tx: &DatabaseTransaction,
    w: &str,
    object: &str,
    mime: &str,
) -> Result<()> {
    if mime.starts_with("audio/") {
        return Ok(());
    }
    execute(
        tx,
        "INSERT INTO relay_object_contents(workspace,object) VALUES($1,$2)",
        vec![w.into(), object.into()],
    )
    .await?;
    Ok(())
}

impl PgStore {
    /// Takes up to `limit` waiting files. Two controllers never take the same
    /// one: rows are locked with SKIP LOCKED and leased in one statement.
    pub(crate) async fn claim_readings(&self, limit: i64) -> Result<Vec<Claimed>> {
        let tx = self.db.begin().await?;
        // The last try's lease ran out (its controller stopped): give up.
        let expired = rows(&tx, &format!("UPDATE relay_object_contents SET status='failed',claim=NULL,finished_ms={NOW_MS}
            WHERE status='pending' AND attempts>={ATTEMPTS} AND lease_until_ms<={NOW_MS} RETURNING workspace,object"), vec![]).await?;
        for row in expired {
            let (w, object): (String, String) =
                (row.try_get("", "workspace")?, row.try_get("", "object")?);
            super::super::transcripts::announce(&tx, &w, &object).await?;
        }
        let claim = uuid::Uuid::new_v4().to_string();
        let taken = rows(&tx, &format!("WITH next AS (SELECT workspace,object FROM relay_object_contents
              WHERE status='pending' AND attempts<{ATTEMPTS} AND lease_until_ms<={NOW_MS}
              ORDER BY created_ms LIMIT $2 FOR UPDATE SKIP LOCKED)
            UPDATE relay_object_contents c SET claim=$1,attempts=c.attempts+1,lease_until_ms={NOW_MS}+{LEASE_MS}
            FROM next, relay_objects o WHERE c.workspace=next.workspace AND c.object=next.object
              AND o.workspace=c.workspace AND o.id=c.object
            RETURNING c.workspace,c.object,o.name,o.mime,o.size,o.checksum"),
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
                })
            })
            .collect()
    }

    /// How long until a waiting file's retry pause or lease ends; None when
    /// none is waiting for a time.
    pub(crate) async fn next_reading_in(&self) -> Result<Option<std::time::Duration>> {
        let wait = query(
            &self.db,
            &format!(
                "SELECT min(lease_until_ms)-{NOW_MS} AS wait FROM relay_object_contents
            WHERE status='pending' AND lease_until_ms>{NOW_MS}"
            ),
            vec![],
        )
        .await?
        .context("File reading queue unavailable")?
        .try_get::<Option<i64>>("", "wait")?;
        Ok(wait.map(|ms| std::time::Duration::from_millis(ms.max(0) as u64)))
    }

    /// Saves what was read from a held file. False when the lease was lost and
    /// another controller took it, so this result is dropped.
    pub(crate) async fn finish_reading(&self, item: &Claimed, outcome: Outcome) -> Result<bool> {
        let tx = self.workspace_tx(&item.workspace).await?;
        let mut args: Vec<sea_orm::Value> = vec![
            item.workspace.clone().into(),
            item.object.clone().into(),
            item.claim.clone().into(),
        ];
        let held = "workspace=$1 AND object=$2 AND claim=$3 AND status='pending'";
        let sql = match outcome {
            Outcome::Read {
                kind,
                text,
                truncated,
            } => {
                let kept: String = text.trim().chars().take(MAX_TEXT).collect();
                let truncated = truncated || kept.chars().count() < text.trim().chars().count();
                args.extend([kind.into(), kept.into(), truncated.into()]);
                format!(
                    "UPDATE relay_object_contents SET status='ready',kind=$4,text=$5,truncated=$6,claim=NULL,finished_ms={NOW_MS} WHERE {held}"
                )
            }
            Outcome::Unreadable { kind } => {
                args.push(kind.into());
                format!(
                    "UPDATE relay_object_contents SET status='unsupported',kind=$4,claim=NULL,finished_ms={NOW_MS} WHERE {held}"
                )
            }
            // Wait a little longer after each try; the last try fails for good.
            Outcome::Retry => format!(
                "UPDATE relay_object_contents SET claim=NULL,
                    status=CASE WHEN attempts>={ATTEMPTS} THEN 'failed' ELSE 'pending' END,
                    finished_ms=CASE WHEN attempts>={ATTEMPTS} THEN {NOW_MS} END,
                    lease_until_ms={NOW_MS}+10000*attempts WHERE {held}"
            ),
        };
        let changed = execute(&tx, &sql, args).await?;
        if changed == 1 {
            let finished = query(&tx, "SELECT status<>'pending' AS finished FROM relay_object_contents WHERE workspace=$1 AND object=$2", vec![item.workspace.clone().into(), item.object.clone().into()])
                .await?.context("File reading missing")?.try_get::<bool>("", "finished")?;
            if finished {
                super::super::transcripts::announce(&tx, &item.workspace, &item.object).await?;
            }
        }
        tx.commit().await?;
        Ok(changed == 1)
    }
}

/// What was read from these files, by file ID.
pub(in crate::identity::postgres) async fn saved(
    tx: &DatabaseTransaction,
    w: &str,
    objects: &[String],
) -> Result<HashMap<String, Reading>> {
    let mut found = HashMap::new();
    if objects.is_empty() {
        return Ok(found);
    }
    for row in rows(tx, "SELECT object,status,kind,text,truncated FROM relay_object_contents WHERE workspace=$1 AND object IN (SELECT jsonb_array_elements_text($2::jsonb))", vec![w.into(), json!(objects).into()]).await? {
        found.insert(
            row.try_get("", "object")?,
            Reading {
                status: row.try_get("", "status")?,
                kind: row.try_get("", "kind")?,
                text: row.try_get("", "text")?,
                truncated: row.try_get("", "truncated")?,
            },
        );
    }
    Ok(found)
}

/// The reading status agents and people are shown for a file: pending, ready,
/// unsupported, failed, or not_read for a file uploaded before schema 100 that
/// was never shared in a message.
pub(in crate::identity::postgres) fn status(reading: Option<&Reading>) -> &str {
    reading.map_or("not_read", |reading| reading.status.as_str())
}

/// Adds `reading` to the non-audio attachments of messages a person may read,
/// so they know whether agents could read each file.
pub(in crate::identity::postgres) async fn for_people(
    tx: &DatabaseTransaction,
    w: &str,
    messages: &mut [Value],
) -> Result<()> {
    let readable = |file: &Value| !file["mime"].as_str().unwrap_or("").starts_with("audio/");
    let objects: Vec<String> = messages
        .iter()
        .flat_map(|m| m["attachments"].as_array().cloned().unwrap_or_default())
        .filter(readable)
        .filter_map(|file| file["id"].as_str().map(str::to_owned))
        .collect();
    let found = saved(tx, w, &objects).await?;
    for message in messages.iter_mut() {
        let Some(files) = message["attachments"].as_array_mut() else {
            continue;
        };
        for file in files.iter_mut().filter(|file| readable(file)) {
            let reading = file["id"].as_str().and_then(|id| found.get(id));
            file["reading"] = json!(status(reading));
        }
    }
    Ok(())
}

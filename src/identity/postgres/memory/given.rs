//! What a request is given from memory without searching, at a fixed size
//! however large the memory grows, and the memory lines of a request in Activity.
use super::tools::{record, shelf_filter};
use super::*;
use serde_json::json;

/// Characters given from each part.
const WORKSPACE: usize = 2000;
const JOB: usize = 1500;
const HERE: usize = 1500;
const TOPICS: usize = 3000;

/// The notes of one shelf as lines, best first, up to `most` characters.
async fn lines(
    db: &impl ConnectionTrait,
    w: &str,
    shelf: &Shelf,
    most: usize,
) -> Result<(Vec<String>, i64)> {
    let found = rows(
        db,
        &format!("SELECT m.id,m.kind,m.topic,m.text FROM relay_memories m
         WHERE m.workspace=$1 AND m.shelf=$2 AND {ACTIVE} AND {STRENGTH}>={ARCHIVE_BELOW}
         ORDER BY m.pinned DESC,m.essential DESC,{STRENGTH} DESC,m.used DESC,m.created_ms DESC LIMIT 80"),
        vec![w.into(), shelf.key().into()],
    )
    .await?;
    let total: i64 = query(
        db,
        &format!("SELECT count(*) AS n FROM relay_memories m WHERE m.workspace=$1 AND m.shelf=$2 AND {ACTIVE}"),
        vec![w.into(), shelf.key().into()],
    )
    .await?
    .context("Missing memory count")?
    .try_get("", "n")?;
    let mut out = vec![];
    let mut used = 0;
    for row in found {
        let id: String = row.try_get("", "id")?;
        let topic: String = row.try_get("", "topic")?;
        let entry = format!(
            "[{}] {}{}: {}",
            name(&id),
            row.try_get::<String>("", "kind")?,
            if topic.is_empty() {
                String::new()
            } else {
                format!(" · {topic}")
            },
            row.try_get::<String>("", "text")?
        );
        if used + entry.len() > most {
            break;
        }
        used += entry.len() + 1;
        out.push(entry);
    }
    let left = total - out.len() as i64;
    Ok((out, left))
}

impl PgStore {
    /// The memory block of a request's instructions; empty when there is none.
    /// Recorded once per request as "given".
    pub(crate) async fn memory_given(&self, job: &Job) -> Result<String> {
        if !ready(&self.db).await? {
            return Ok(String::new());
        }
        let tx = self.workspace_tx(&job.workspace).await?;
        let access = access(&tx, job).await?;
        let w = &job.workspace;
        let mut parts = vec![];
        let mut given = 0;
        let mut left = 0;
        for shelf in access.read.iter().rev() {
            let (most, title) = match shelf.scope {
                "workspace" => (WORKSPACE, "The workspace (every agent knows these)"),
                "job" => (JOB, "Your job (what you learned about your own work)"),
                "room" => (HERE, "This conversation (only used here)"),
                _ => (
                    HERE,
                    "This person (only used in their own conversation with you)",
                ),
            };
            let (shown, more) = lines(&tx, w, shelf, most).await?;
            given += shown.len();
            left += more;
            if !shown.is_empty() {
                parts.push(format!("{title}:\n{}", shown.join("\n")));
            }
        }
        // Topic summaries that match what was asked.
        let asked: String = query(
            &tx,
            "SELECT COALESCE(m.content,'') AS content FROM relay_chat_jobs j LEFT JOIN relay_chat_messages m ON m.workspace=j.workspace AND m.id=j.request_message WHERE j.workspace=$1 AND j.reply=$2",
            vec![w.clone().into(), job.reply.clone().into()],
        )
        .await?
        .map(|r| r.try_get("", "content"))
        .transpose()?
        .unwrap_or_default();
        if let Some(words) = any_words(&asked.chars().take(2000).collect::<String>(), 24) {
            let (filter, shelves) = shelf_filter("t", &access.read, 3);
            let mut args: Vec<sea_orm::Value> = vec![w.clone().into(), words.into()];
            args.extend(shelves);
            let topics = rows(
                &tx,
                &format!("SELECT t.topic,t.summary,t.scope FROM relay_memory_topics t,to_tsquery('simple',$2) q
                 WHERE t.workspace=$1 AND {filter} AND to_tsvector('simple',t.topic||' '||t.summary) @@ q
                 ORDER BY ts_rank(to_tsvector('simple',t.topic||' '||t.summary),q) DESC LIMIT 3"),
                args,
            )
            .await?;
            let mut used = 0;
            let mut shown = vec![];
            for row in topics {
                let scope: String = row.try_get("", "scope")?;
                let from = match scope.as_str() {
                    "workspace" => "workspace",
                    "job" => "your job",
                    _ => "this conversation",
                };
                let entry = format!(
                    "{} ({from}): {}",
                    row.try_get::<String>("", "topic")?,
                    row.try_get::<String>("", "summary")?
                );
                if used + entry.len() > TOPICS {
                    break;
                }
                used += entry.len();
                shown.push(entry);
            }
            given += shown.len();
            if !shown.is_empty() {
                parts.push(format!(
                    "Topics that match this request:\n{}",
                    shown.join("\n")
                ));
            }
        }
        if given > 0 {
            record(&tx, job, "given", None, Some(given as i64), None).await?;
        }
        tx.commit().await?;
        if parts.is_empty() {
            return Ok(String::new());
        }
        let mut text = format!(
            "{}{}",
            crate::identity::agents::system_prompt::MEMORY_NOTES,
            parts.join("\n\n")
        );
        if left > 0 {
            text.push_str(&format!(
                "\n{left} more notes are not shown: find them with search_memory."
            ));
        }
        Ok(text)
    }
}

/// The memory lines of one request in Activity, in the order they happened.
pub(crate) async fn steps(db: &impl ConnectionTrait, w: &str, reply: &str) -> Result<Vec<Value>> {
    if !ready(db).await? {
        return Ok(vec![]);
    }
    let found = rows(
        db,
        "SELECT u.action,u.query,u.found,u.at_ms,m.id,m.scope,m.text,m.state,p.display_name AS person
         FROM relay_memory_uses u
         LEFT JOIN relay_memories m ON m.workspace=u.workspace AND m.id=u.memory
         LEFT JOIN relay_users p ON p.id=m.user_id
         WHERE u.workspace=$1 AND u.job=$2 ORDER BY u.at_ms LIMIT 60",
        vec![w.into(), reply.into()],
    )
    .await?;
    let mut steps = vec![];
    for (i, row) in found.into_iter().enumerate() {
        let action: String = row.try_get("", "action")?;
        let asked = row
            .try_get::<Option<String>>("", "query")?
            .unwrap_or_default();
        let count = row.try_get::<Option<i32>>("", "found")?.unwrap_or(0);
        let text = row.try_get::<Option<String>>("", "text")?;
        let target = match row.try_get::<Option<String>>("", "scope")?.as_deref() {
            Some("workspace") => "for the workspace".to_owned(),
            Some("job") => "for its job".to_owned(),
            Some("room") => "for this conversation".to_owned(),
            Some(_) => format!(
                "for {}",
                row.try_get::<Option<String>>("", "person")?
                    .unwrap_or_else(|| "this person".into())
            ),
            None => String::new(),
        };
        let note = |verb: &str| match &text {
            Some(text) => format!("{verb} {target}: {text}"),
            None => format!("{verb} a note (since removed)"),
        };
        let title = match action.as_str() {
            "given" => format!(
                "Given from memory: {count} {}",
                if count == 1 { "note" } else { "notes" }
            ),
            "searched" => format!("Searched memory: \"{asked}\" · {count} found"),
            "searched_history" => {
                format!("Searched this conversation: \"{asked}\" · {count} found")
            }
            "kept" => note("Remembered"),
            "suggested" => note("Suggested, waiting for a person,"),
            _ => match &text {
                Some(text) => format!("Ended a note: {text}"),
                None => "Ended a note (since removed)".into(),
            },
        };
        steps.push(json!({"id":format!("memory-{i}-{}",row.try_get::<i64>("", "at_ms")?),"title":title,
            "effect":"memory","state":"completed","automatic":true,"decision":null,"decided_by":null,"account":null,
            "arguments":{},"at":row.try_get::<i64>("", "at_ms")?/1000}));
    }
    Ok(steps)
}

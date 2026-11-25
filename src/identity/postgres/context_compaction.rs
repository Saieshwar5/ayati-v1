//! Automatic working summaries (schema 120). When a room's conversation grows
//! past what one prompt holds (`history_budget`: a share of the agent model's
//! context window), Cuckoo writes the room's working summary itself,
//! in the background after the agent's work finishes. It reads exactly what the
//! agent's own history would show in that room, and nothing from other rooms:
//! one person's private conversation or a shared conversation's single brief.
//! Recent messages still reach the agent word
//! for word; the summary carries what scrolled out of view. Like the agent's own
//! summaries it is an untrusted interpretation that cites its messages.
use super::working_context::Brief;
use super::*;
use crate::identity::chat::model::Job;
use serde_json::json;
use std::collections::BTreeSet;

/// Too few new messages to be worth a summary, however long they are.
const FEWEST: usize = 6;
/// The most new messages one check reads.
const READ_MOST: i64 = 400;
/// The most one summary is given to read. A room further behind (its summaries
/// failed for a while) catches up oldest first, one summary per finished reply.
const INPUT_BYTES: usize = 128_000;
/// Each message's text is cut to this many characters for the summarizer.
const MESSAGE_CHARS: usize = 2_000;

/// One finished reply, and the room (workspace, agent, conversation) it is in.
pub(crate) struct Finished {
    pub workspace: String,
    pub reply: String,
    pub at_ms: i64,
    pub room: (String, String, String),
}

/// What the summarizer reads for one room, and what saving checks again.
pub(crate) struct CompactionInput {
    pub job: Job,
    claim: String,
    generation: Value,
    revision: i64,
    cutoff: i64,
    /// The summary to carry forward, when it is still current.
    pub previous: Option<Value>,
    /// New messages, oldest first: id, author, when, thread and text.
    pub messages: Vec<Value>,
    /// Everyone in the room reads this summary.
    pub shared: bool,
    pub person: String,
}

impl CompactionInput {
    /// The message IDs a new summary may cite: the messages read now and those
    /// the summary it carries forward already cited.
    pub(crate) fn eligible(&self) -> BTreeSet<String> {
        let mut ids: BTreeSet<String> = self
            .messages
            .iter()
            .filter_map(|m| m["id"].as_str().map(str::to_owned))
            .collect();
        if let Some(Ok(brief)) = self
            .previous
            .as_ref()
            .map(|previous| serde_json::from_value::<Brief>(previous.clone()))
        {
            ids.extend(brief.sources());
        }
        ids
    }
}

impl PgStore {
    /// Finished work after `after`, oldest first, across workspaces.
    pub(crate) async fn finished_since(
        &self,
        after: &(i64, String),
        limit: i64,
    ) -> Result<Vec<Finished>> {
        rows(&self.db, "SELECT workspace,reply,finished_ms,payload->>'agent' AS agent,payload->>'chat' AS chat FROM relay_chat_jobs
            WHERE state='finished' AND finished_ms IS NOT NULL AND (finished_ms,reply)>($1,$2) ORDER BY finished_ms,reply LIMIT $3",
            vec![after.0.into(), after.1.clone().into(), limit.into()]).await?
            .iter()
            .map(|r| Ok(Finished {
                workspace: r.try_get("", "workspace")?,
                reply: r.try_get("", "reply")?,
                at_ms: r.try_get("", "finished_ms")?,
                room: (r.try_get("", "workspace")?, r.try_get::<Option<String>>("", "agent")?.unwrap_or_default(),
                    r.try_get::<Option<String>>("", "chat")?.unwrap_or_default()),
            }))
            .collect()
    }

    /// The room a finished reply belongs to, when its conversation has grown
    /// enough since the last summary to need a new one.
    pub(crate) async fn compaction_input(
        &self,
        w: &str,
        reply: &str,
    ) -> Result<Option<CompactionInput>> {
        let tx = self.read_tx(w).await?;
        let input = input(&tx, w, reply, |model| self.model_window(model)).await;
        tx.commit().await?;
        input
    }

    /// Saves the summary as the room's next version, unless the room changed
    /// while it was written: access or sources (generation), or a newer
    /// summary saved by the agent or another controller. Then it is dropped.
    pub(crate) async fn save_compaction(
        &self,
        input: &CompactionInput,
        answer: &Value,
        model: &str,
    ) -> Result<bool> {
        let Some(mut brief) = Brief::from_model(input.revision, answer, &input.eligible()) else {
            return Ok(false);
        };
        let job = &input.job;
        let tx = self.workspace_tx(&job.workspace).await?;
        let shared = super::shared_rooms::audience(&tx, &job.workspace, &job.chat)
            .await?
            .is_some();
        brief.fade_roles(&tx, &job.workspace, shared).await?;
        if super::context_attempts::generation(&tx, job).await? != input.generation
            || super::working_context::latest(&tx, job).await?["revision"].as_i64()
                != Some(input.revision)
        {
            tx.commit().await?;
            return Ok(false);
        }
        let owner = super::shared_rooms::brief_owner(&tx, job).await?;
        let covered: Vec<&Value> = input.messages.iter().map(|m| &m["id"]).collect();
        let saved = execute(&tx, "INSERT INTO relay_working_context(workspace,agent,chat,requester,revision,task,claim,generation,cutoff,brief,made_by,model,covered)
            VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,'automatic',$11,$12) ON CONFLICT DO NOTHING",
            vec![job.workspace.clone().into(), job.agent.clone().into(), job.chat.clone().into(), owner.into(), (input.revision + 1).into(),
                job.reply.clone().into(), input.claim.clone().into(), input.generation.clone().into(), input.cutoff.into(),
                serde_json::to_value(&brief)?.into(), model.into(), json!(covered).into()]).await?;
        tx.commit().await?;
        Ok(saved == 1)
    }
}

/// Whether new messages of this size are due a summary: they are about to
/// leave the prompt's history window (most of `budget` bytes, or most of the
/// messages it reads).
fn due(count: usize, bytes: usize, budget: usize) -> bool {
    count >= FEWEST
        && (bytes > budget * 6 / 7
            || count as i64 >= super::history_budget::messages(budget) * 2 / 3)
}

async fn input(
    tx: &DatabaseTransaction,
    w: &str,
    reply: &str,
    window: impl Fn(&str) -> u32,
) -> Result<Option<CompactionInput>> {
    if !super::context_attempts::exists(tx).await? {
        return Ok(None);
    }
    // Only finished, complete replies of managed agents, whose context was saved.
    let Some(row) = query(tx, "SELECT j.payload,m.sequence,a.settings,a.model,
            (SELECT c.claim FROM relay_context_attempts c WHERE c.workspace=j.workspace AND c.task=j.reply ORDER BY c.created_at DESC,c.claim LIMIT 1) AS claim
         FROM relay_chat_jobs j
         JOIN relay_chat_messages m ON m.workspace=j.workspace AND m.id=j.reply AND m.state='complete'
         JOIN relay_shared_agents a ON a.workspace=j.workspace AND a.id=j.payload->>'agent' AND a.revision>0
         WHERE j.workspace=$1 AND j.reply=$2 AND j.state='finished'", vec![w.into(), reply.into()]).await? else {
        return Ok(None);
    };
    let Some(claim) = row.try_get::<Option<String>>("", "claim")? else {
        return Ok(None);
    };
    let job: Job = serde_json::from_value(row.try_get("", "payload")?)?;
    // A paused workspace or a spent budget makes no model calls for its agents.
    if super::settings::pause::paused(tx, w).await?
        || super::model_budgets::budget_reached(tx, w, &job.agent)
            .await?
            .is_some()
    {
        return Ok(None);
    }
    let reply_sequence: i64 = row.try_get("", "sequence")?;
    let budget = super::history_budget::bytes(window(&row.try_get::<String>("", "model")?));
    let latest = super::working_context::latest(tx, &job).await?;
    let revision = latest["revision"].as_i64().unwrap_or(0);
    // A summary the room's access or sources have since outdated is not carried
    // forward: the new one starts from the messages alone.
    let (previous, after) = if latest["available"] == true {
        (
            Some(latest["brief"].clone()),
            latest["cutoff"].as_i64().unwrap_or(0),
        )
    } else {
        (None, 0)
    };
    if after >= reply_sequence {
        return Ok(None);
    }
    let settings: Value = row.try_get("", "settings")?;
    let scope = super::conversation_context::RecentScope {
        workspace: w,
        chat: &job.chat,
        agent: &job.agent,
        thread: None,
        main_only: false,
        since_join: settings["history"] == "since_join",
        limit: READ_MOST,
        before: reply_sequence + 1,
        request: "",
    };
    let mut messages = super::conversation_context::gather(tx, scope).await?;
    messages.retain(|m| m["sequence"].as_i64().is_some_and(|s| s > after));
    if !due(messages.len(), serde_json::to_vec(&messages)?.len(), budget) {
        return Ok(None);
    }
    // Oldest first, up to what one summary reads; the summary then covers up to
    // the last message it read.
    let (mut read, mut size, mut cutoff) = (Vec::new(), 0, after);
    for message in &messages {
        let plain = plain(message);
        size += serde_json::to_vec(&plain)?.len() + 1;
        if size > INPUT_BYTES && !read.is_empty() {
            break;
        }
        cutoff = message["sequence"].as_i64().unwrap_or(cutoff);
        read.push(plain);
    }
    let messages = read;
    let owner = super::shared_rooms::brief_owner(tx, &job).await?;
    let generation = super::context_attempts::generation(tx, &job).await?;
    Ok(Some(CompactionInput {
        person: job.requester.clone(),
        shared: owner.is_empty(),
        job,
        claim,
        generation,
        revision,
        cutoff,
        previous,
        messages,
    }))
}

/// One message as the summarizer reads it.
fn plain(message: &Value) -> Value {
    let text: String = message["content"]
        .as_str()
        .unwrap_or("")
        .chars()
        .take(MESSAGE_CHARS)
        .collect();
    let mut plain = json!({
        "id": message["id"],
        "author": message["author"]["display_name"],
        "kind": message["author"]["kind"],
        "at": message["at"],
        "text": text,
    });
    if !message["thread"].is_null() {
        plain["thread"] = message["thread"].clone();
    }
    if let Some(files) = message["attachments"].as_array().filter(|a| !a.is_empty()) {
        plain["files"] = json!(
            files
                .iter()
                .filter_map(|f| f["name"].as_str())
                .collect::<Vec<_>>()
        );
    }
    plain
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_summary_is_due_when_new_messages_are_about_to_leave_the_window() {
        let floor = super::super::history_budget::FLOOR;
        assert!(!super::due(5, 50_000, floor), "too few messages");
        assert!(!super::due(20, 11_000, floor));
        assert!(super::due(20, 12_500, floor), "most of a 14 KB window");
        assert!(
            super::due(40, 4_000, floor),
            "most of the 60 messages it reads"
        );
        // A 128,000-token model carries 64,000 bytes, so it waits much longer.
        assert!(!super::due(60, 30_000, 64_000));
        assert!(super::due(60, 56_000, 64_000));
    }
}

pub(crate) use migration::ContextCompaction;
mod migration {
    use sea_orm_migration::prelude::*;
    pub(crate) struct ContextCompaction;
    impl MigrationName for ContextCompaction {
        fn name(&self) -> &str {
            "m20261115_000120_context_compaction"
        }
    }
    #[async_trait::async_trait]
    impl MigrationTrait for ContextCompaction {
        async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
            m.get_connection()
                .execute_unprepared(include_str!("context_compaction.sql"))
                .await?;
            Ok(())
        }
        async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
            Err(DbErr::Custom(
                "Automatic working summaries require a forward migration".into(),
            ))
        }
    }
}

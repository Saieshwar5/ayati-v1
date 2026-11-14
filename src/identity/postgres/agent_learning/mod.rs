//! Agents learn from their work (schema 121, knowledge K1). Once a day per
//! agent, Cuckoo reads signals from its work in shared conversations (channels
//! and groups of two or more people; never private chats, work rooms or
//! anything from outside): how people answered its replies, their reactions,
//! and approvals they rejected. A model proposes up to five lessons about how
//! to work, which are filtered for personal details and must cite the signals
//! they come from. People who maintain the agent approve, edit or reject them
//! in the Knowledge tab; only an approved lesson becomes a note the agent uses.
use super::*;
use serde_json::json;
use std::collections::BTreeMap;
mod review;
pub(crate) use review::Decision;
#[cfg(test)]
mod tests;
pub(crate) use migration::AgentLearning;

/// At most one run a day per agent.
const EVERY_MS: i64 = 20 * 3_600_000;
/// A first run, or one after a long pause, reads at most this far back.
const LOOK_BACK_MS: i64 = 7 * 24 * 3_600_000;
/// Proposals waiting for review stop new ones until someone looks.
const MOST_OPEN: i64 = 20;
const LESSONS_PER_RUN: usize = 5;
const EXCERPT: i64 = 800;
/// Only channels and groups people share: never a private chat or work room.
const SHARED: &str = "c.kind<>'direct' AND NOT c.work AND (c.visibility='public' OR (SELECT count(*) FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id)>1)";

pub(crate) async fn ready(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regclass('relay_knowledge_proposals') IS NOT NULL AS ready",
        vec![],
    )
    .await?
    .context("Missing schema check")?
    .try_get("", "ready")?)
}

/// What one agent's learning run reads.
pub(crate) struct Signals {
    pub workspace: String,
    pub agent: String,
    pub name: String,
    pub role: String,
    pub until_ms: i64,
    /// Signals as the model reads them: `s1`, `s2`… with what was said.
    pub items: Vec<Value>,
    /// The agent-wide notes it already has, which a lesson may update.
    pub notes: Vec<Value>,
    /// Signal ID → (kind, conversation, message).
    refs: BTreeMap<String, (String, String, String)>,
    /// Names of the workspace's people, never allowed in a lesson.
    names: Vec<String>,
}

/// One lesson as the model proposed it.
pub(crate) struct Lesson {
    pub title: String,
    pub when: String,
    pub what: String,
    pub why: String,
    pub evidence: Vec<String>,
    pub kind: String,
    pub replaces: Option<String>,
}

impl PgStore {
    /// Agents whose daily run is due and who did any work since the last one.
    pub(crate) async fn learning_due(
        &self,
        now_ms: i64,
        limit: i64,
    ) -> Result<Vec<(String, String)>> {
        if !ready(&self.db).await? {
            return Ok(vec![]);
        }
        rows(&self.db, "SELECT a.workspace,a.id FROM relay_shared_agents a
            LEFT JOIN relay_agent_learning l ON l.workspace=a.workspace AND l.agent=a.id
            WHERE a.revision>0 AND COALESCE(l.enabled,true) AND (l.last_run_ms IS NULL OR l.last_run_ms<$1-$2)
            AND EXISTS(SELECT 1 FROM relay_chat_messages m WHERE m.workspace=a.workspace AND m.author=a.id AND m.kind='agent'
              AND m.at*1000>GREATEST(COALESCE(l.learned_until_ms,0),$1-$3))
            ORDER BY COALESCE(l.last_run_ms,0),a.workspace,a.id LIMIT $4",
            vec![now_ms.into(), EVERY_MS.into(), LOOK_BACK_MS.into(), limit.into()]).await?
            .iter()
            .map(|r| Ok((r.try_get("", "workspace")?, r.try_get("", "id")?)))
            .collect()
    }

    /// The signals since the agent's last run, or `None` when there is nothing
    /// to read (it is then marked as run) or proposals are waiting for review.
    pub(crate) async fn learning_signals(
        &self,
        w: &str,
        agent: &str,
        now_ms: i64,
    ) -> Result<Option<Signals>> {
        let tx = self.read_tx(w).await?;
        let signals = signals(&tx, w, agent, now_ms).await;
        tx.commit().await?;
        match signals? {
            Ok(signals) if signals.items.is_empty() => {
                self.learning_ran(
                    w,
                    agent,
                    now_ms,
                    signals.until_ms,
                    "Nothing new to learn from",
                )
                .await?;
                Ok(None)
            }
            Ok(signals) => Ok(Some(signals)),
            // Tried again tomorrow, from where the last run stopped.
            Err(reason) => {
                self.learning_ran(w, agent, now_ms, 0, reason).await?;
                Ok(None)
            }
        }
    }

    /// Saves the lessons that pass the checks and records the run.
    pub(crate) async fn save_lessons(
        &self,
        signals: &Signals,
        lessons: Vec<Lesson>,
        model: &str,
    ) -> Result<usize> {
        let tx = self.workspace_tx(&signals.workspace).await?;
        let mut saved = 0;
        for lesson in lessons.into_iter().take(LESSONS_PER_RUN) {
            let Some(row) = checked(signals, lesson) else {
                continue;
            };
            execute(&tx, "INSERT INTO relay_knowledge_proposals(workspace,agent,id,chat,title,content,why,evidence,kind,replaces,model)
                SELECT $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11 FROM relay_shared_agents WHERE workspace=$1 AND id=$2",
                vec![signals.workspace.clone().into(), signals.agent.clone().into(), uuid::Uuid::new_v4().to_string().into(),
                    row.chat.into(), row.title.into(), row.content.into(), row.why.into(), row.evidence.into(),
                    row.kind.into(), row.replaces.into(), model.into()]).await?;
            saved += 1;
        }
        tx.commit().await?;
        let result = match saved {
            0 => "Nothing worth a lesson".to_owned(),
            1 => "1 lesson to review".to_owned(),
            n => format!("{n} lessons to review"),
        };
        self.learning_ran(
            &signals.workspace,
            &signals.agent,
            signals.until_ms,
            signals.until_ms,
            &result,
        )
        .await?;
        Ok(saved)
    }

    /// Records a run, so the next one starts where this one stopped.
    pub(crate) async fn learning_ran(
        &self,
        w: &str,
        agent: &str,
        now_ms: i64,
        until_ms: i64,
        result: &str,
    ) -> Result<()> {
        execute(&self.db, "INSERT INTO relay_agent_learning(workspace,agent,learned_until_ms,last_run_ms,last_result) VALUES($1,$2,$3,$4,$5)
            ON CONFLICT(workspace,agent) DO UPDATE SET learned_until_ms=GREATEST(relay_agent_learning.learned_until_ms,excluded.learned_until_ms),
            last_run_ms=excluded.last_run_ms,last_result=excluded.last_result",
            vec![w.into(), agent.into(), until_ms.into(), now_ms.into(), result.into()]).await?;
        Ok(())
    }
}

/// The signals to read, or why this agent learns nothing today.
async fn signals(
    tx: &DatabaseTransaction,
    w: &str,
    agent: &str,
    now_ms: i64,
) -> Result<std::result::Result<Signals, &'static str>> {
    let Some(profile) = query(tx, "SELECT a.name,a.role,COALESCE(l.learned_until_ms,0) AS until,
            (SELECT count(*) FROM relay_knowledge_proposals k WHERE k.workspace=a.workspace AND k.agent=a.id AND k.state='proposed') AS open
         FROM relay_shared_agents a LEFT JOIN relay_agent_learning l ON l.workspace=a.workspace AND l.agent=a.id
         WHERE a.workspace=$1 AND a.id=$2 AND a.revision>0", vec![w.into(), agent.into()]).await? else {
        return Ok(Err("Not a managed agent"));
    };
    if profile.try_get::<i64>("", "open")? >= MOST_OPEN {
        return Ok(Err("Waiting for earlier lessons to be reviewed"));
    }
    if super::settings::pause::paused(tx, w).await? {
        return Ok(Err("Agents are paused"));
    }
    if super::model_budgets::budget_reached(tx, w, agent)
        .await?
        .is_some()
    {
        return Ok(Err("The model budget is used up"));
    }
    let since = profile
        .try_get::<i64>("", "until")?
        .max(now_ms - LOOK_BACK_MS);
    // Replies built from restricted sources (connected events, Library sources,
    // hand-off trees…) stay out, exactly as they stay out of later prompts.
    let exclusions = super::conversation_context::source_exclusions(tx).await?;
    let args = || {
        vec![
            w.into(),
            agent.into(),
            since.into(),
            now_ms.into(),
            EXCERPT.into(),
        ]
    };
    let mut follow_ups = json_rows(tx, &format!("SELECT jsonb_build_object('kind','answer','chat',m.chat,'message',m.id,'conversation',c.name,
            'agent_said',left(m.content,$5::int),'person_said',left(f.content,$5::int)) AS data
        FROM relay_chat_messages m JOIN relay_chats c ON c.workspace=m.workspace AND c.id=m.chat
        JOIN LATERAL (SELECT f.content FROM relay_chat_messages f WHERE f.workspace=m.workspace AND f.chat=m.chat AND f.kind='user'
          AND f.sequence>m.sequence AND f.thread IS NOT DISTINCT FROM m.thread AND f.state IN ('sent','complete') AND f.at<=m.at+21600
          ORDER BY f.sequence LIMIT 1) f ON TRUE
        WHERE m.workspace=$1 AND m.author=$2 AND m.kind='agent' AND m.state='complete' AND m.at*1000>$3 AND m.at*1000<=$4
          AND {SHARED}{exclusions} ORDER BY m.sequence DESC LIMIT 40"), args()).await?;
    let mut reactions = json_rows(tx, &format!("SELECT jsonb_build_object('kind','reaction','chat',m.chat,'message',m.id,'conversation',c.name,
            'agent_said',left(m.content,$5::int),'reaction',r.emoji) AS data
        FROM relay_message_reactions r JOIN relay_chat_messages m ON m.workspace=r.workspace AND m.id=r.message
        JOIN relay_chats c ON c.workspace=m.workspace AND c.id=m.chat
        WHERE r.workspace=$1 AND m.author=$2 AND m.kind='agent' AND r.at*1000>$3 AND r.at*1000<=$4
          AND {SHARED}{exclusions} ORDER BY r.at DESC LIMIT 30"), args()).await?;
    let rejected = json_rows(tx, &format!("SELECT jsonb_build_object('kind','rejected_approval','chat',x.chat,'message',x.job,'conversation',c.name,
            'agent_asked_to',left(x.title,$5::int)) AS data
        FROM relay_control_actions x JOIN relay_chats c ON c.workspace=x.workspace AND c.id=x.chat
        WHERE x.workspace=$1 AND x.agent=$2 AND x.state='rejected' AND x.decided_at*1000>$3 AND x.decided_at*1000<=$4
          AND {SHARED} ORDER BY x.decided_at DESC LIMIT 20"), args()).await?;
    // The newest were read; the model reads them in the order they happened.
    follow_ups.reverse();
    reactions.reverse();
    let mut items = Vec::new();
    let mut refs = BTreeMap::new();
    for (n, mut signal) in follow_ups
        .into_iter()
        .chain(reactions)
        .chain(rejected)
        .enumerate()
    {
        let id = format!("s{}", n + 1);
        let field = |key: &str| signal[key].as_str().unwrap_or_default().to_owned();
        refs.insert(id.clone(), (field("kind"), field("chat"), field("message")));
        if let Some(fields) = signal.as_object_mut() {
            fields.remove("chat");
            fields.remove("message");
            fields.insert("id".into(), json!(id));
        }
        items.push(signal);
    }
    let notes = json_rows(tx, "SELECT jsonb_build_object('id',id,'title',title,'content',left(content,600)) AS data FROM relay_agent_knowledge
        WHERE workspace=$1 AND agent=$2 AND chat IS NULL AND NOT forgotten ORDER BY created_at LIMIT 40", vec![w.into(), agent.into()]).await?;
    let names = rows(
        tx,
        "SELECT DISTINCT u.display_name FROM relay_members p JOIN relay_users u ON u.id=p.user_id
        WHERE p.workspace=$1 AND u.display_name IS NOT NULL",
        vec![w.into()],
    )
    .await?
    .iter()
    .filter_map(|r| r.try_get::<String>("", "display_name").ok())
    .collect();
    Ok(Ok(Signals {
        workspace: w.into(),
        agent: agent.into(),
        name: profile.try_get("", "name")?,
        role: profile.try_get("", "role")?,
        until_ms: now_ms,
        items,
        notes,
        refs,
        names,
    }))
}

struct Checked {
    chat: Option<String>,
    title: String,
    content: String,
    why: String,
    evidence: Value,
    kind: String,
    replaces: Option<String>,
}

/// A lesson that cites only signals it was given, updates only a note the
/// agent has, and names no person, address, number or amount.
fn checked(signals: &Signals, lesson: Lesson) -> Option<Checked> {
    let cut = |text: &str, most: usize| text.trim().chars().take(most).collect::<String>();
    let (title, when, what, why) = (
        cut(&lesson.title, 120),
        cut(&lesson.when, 400),
        cut(&lesson.what, 600),
        cut(&lesson.why, 600),
    );
    if [&title, &when, &what].iter().any(|t| t.is_empty())
        || [&title, &when, &what, &why]
            .iter()
            .any(|t| personal(t, &signals.names))
    {
        return None;
    }
    let evidence: Vec<&(String, String, String)> = lesson
        .evidence
        .iter()
        .filter_map(|id| signals.refs.get(id))
        .take(6)
        .collect();
    if evidence.is_empty() {
        return None;
    }
    let kind = match lesson.kind.as_str() {
        "update" | "conflict" => lesson.kind.clone(),
        _ => "new".into(),
    };
    let replaces = lesson
        .replaces
        .filter(|id| signals.notes.iter().any(|n| n["id"] == id.as_str()));
    if kind != "new" && replaces.is_none() {
        return None;
    }
    let first = &evidence[0].1;
    Some(Checked {
        chat: evidence
            .iter()
            .all(|e| &e.1 == first)
            .then(|| first.clone()),
        title,
        content: format!("When: {when}\nDo: {what}"),
        why,
        evidence: json!(
            evidence
                .iter()
                .map(|(kind, chat, message)| json!({"kind":kind,"chat":chat,"message":message}))
                .collect::<Vec<_>>()
        ),
        kind,
        replaces,
    })
}

/// Whether text names someone or carries contact details, long numbers or
/// amounts: lessons are about how to work, never about one person or case.
pub(crate) fn personal(text: &str, names: &[String]) -> bool {
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let digits_run = text
        .split(|c: char| !c.is_ascii_digit())
        .any(|run| run.len() >= 5);
    let email = text
        .split_whitespace()
        .any(|w| w.contains('@') && w.contains('.'));
    let money = ["₹", "$", "€", "£", "rs.", "inr "].iter().any(|sign| {
        lower.match_indices(sign).any(|(i, s)| {
            lower[i + s.len()..]
                .trim_start()
                .starts_with(|c: char| c.is_ascii_digit())
        })
    });
    let named = names.iter().any(|name| {
        let name = name.to_lowercase();
        let parts: Vec<&str> = name
            .split(|c: char| !c.is_alphanumeric())
            .filter(|p| p.len() >= 3)
            .collect();
        parts.first().is_some_and(|first| words.contains(first))
            || (parts.len() > 1 && lower.contains(&name))
    });
    digits_run || email || money || named
}

mod migration {
    use sea_orm_migration::prelude::*;
    pub(crate) struct AgentLearning;
    impl MigrationName for AgentLearning {
        fn name(&self) -> &str {
            "m20261115_000121_agent_learning"
        }
    }
    #[async_trait::async_trait]
    impl MigrationTrait for AgentLearning {
        async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
            m.get_connection()
                .execute_unprepared(include_str!("learning.sql"))
                .await?;
            Ok(())
        }
        async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
            Err(DbErr::Custom(
                "Agent learning requires a forward migration".into(),
            ))
        }
    }
}

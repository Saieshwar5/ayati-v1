//! Memory (schema 142): what agents learn while working, kept on four kinds of
//! shelf with one engine.
//!
//! - **Workspace:** facts about the business. Every shared agent reads it, an
//!   assistant reads it too; only admins open and change it.
//! - **Job:** what one agent learned about its own work, in every conversation.
//! - **Room:** what one agent learned in one channel, group, work room or
//!   outside thread, used only there.
//! - **Person:** what one agent learned with one person in their private
//!   conversation, used only when that person asks there.
//!
//! The controller decides from the running task which shelves a request may
//! read and the one it may write to directly ([`access`]); no tool argument
//! names a person, room or agent. A note filed above that shelf from a place
//! the whole workspace cannot see waits for a person. Notes are information,
//! never instructions.
use super::*;
use crate::identity::chat::model::Job;

pub(crate) mod changes;
pub(crate) mod given;
pub(crate) mod manage;
#[cfg(test)]
mod manage_tests;
pub(crate) mod notes;
#[cfg(test)]
mod tests;
pub(crate) mod tidy;
#[cfg(test)]
mod tidy_tests;
pub(crate) mod tools;

/// What a note is about. The kind sets how fast an unused note fades.
pub(crate) const KINDS: [&str; 8] = [
    "customer",
    "system",
    "people",
    "decision",
    "practice",
    "preference",
    "status",
    "term",
];
/// Strength of a note an agent kept, and of one an admin or owner wrote or confirmed.
pub(crate) const AGENT_STRENGTH: i32 = 50;
pub(crate) const CONFIRMED_STRENGTH: i32 = 90;
/// Below this a note is archived by the tidy-up; below `FADING` it is listed as fading.
pub(crate) const ARCHIVE_BELOW: i32 = 10;
pub(crate) const FADING_BELOW: i32 = 30;
/// The most current notes one shelf holds, by scope.
pub(crate) fn most(scope: &str) -> i64 {
    match scope {
        "workspace" => 50_000,
        "job" => 2_000,
        "room" => 1_000,
        _ => 500,
    }
}
pub(crate) const NOW_MS: &str = "(extract(epoch FROM clock_timestamp())*1000)::bigint";
/// A note of the row `m` agents may use now: current, and not past its date.
pub(crate) const ACTIVE: &str = "m.state='active' AND (m.until_ms IS NULL OR m.until_ms>(extract(epoch FROM clock_timestamp())*1000)::bigint)";
/// A note's strength now, from the row `m`.
pub(crate) const STRENGTH: &str = "relay_memory_strength(m.strength,m.kind,m.used_ms,m.pinned,(extract(epoch FROM clock_timestamp())*1000)::bigint)";

pub(crate) async fn ready(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regclass('relay_memories') IS NOT NULL AS ready",
        vec![],
    )
    .await?
    .context("Missing schema check")?
    .try_get("", "ready")?)
}

/// One shelf: its scope and the agent, room or person it belongs to.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Shelf {
    pub scope: &'static str,
    pub agent: Option<String>,
    pub chat: Option<String>,
    pub user: Option<String>,
}

impl Shelf {
    pub(crate) fn workspace() -> Self {
        Self {
            scope: "workspace",
            agent: None,
            chat: None,
            user: None,
        }
    }
    pub(crate) fn job(agent: &str) -> Self {
        Self {
            scope: "job",
            agent: Some(agent.to_owned()),
            chat: None,
            user: None,
        }
    }
    pub(crate) fn room(agent: &str, chat: &str) -> Self {
        Self {
            scope: "room",
            agent: Some(agent.to_owned()),
            chat: Some(chat.to_owned()),
            user: None,
        }
    }
    pub(crate) fn person(agent: &str, user: &str) -> Self {
        Self {
            scope: "person",
            agent: Some(agent.to_owned()),
            chat: None,
            user: Some(user.to_owned()),
        }
    }
    /// The stored `shelf` column of this shelf's rows.
    pub(crate) fn key(&self) -> String {
        format!(
            "{}:{}:{}:{}",
            self.scope,
            self.agent.as_deref().unwrap_or(""),
            self.chat.as_deref().unwrap_or(""),
            self.user.as_deref().unwrap_or("")
        )
    }
    /// How an agent sees where a note comes from.
    pub(crate) fn label(&self) -> &'static str {
        match self.scope {
            "workspace" => "workspace",
            "job" => "my job",
            _ => "this conversation",
        }
    }
}

/// What one running request may do with memory.
#[derive(Debug)]
pub(crate) struct Access {
    /// The agent is a person's own assistant: it reads the workspace's
    /// memory and keeps its own memory of its person elsewhere.
    pub assistant: bool,
    /// Shelves this request reads, nearest first.
    pub read: Vec<Shelf>,
    /// The shelf a note is kept on when the agent keeps it "here".
    pub here: Option<Shelf>,
    /// Everyone in the workspace may read this conversation, the request
    /// came from a person and used no restricted source: what is learned
    /// here may go on the job and workspace shelves directly.
    pub open: bool,
    /// Why a note filed wider waits for a person, when it does.
    pub waits_because: Option<&'static str>,
    /// Built from restricted sources (Library, connected events): nothing is kept.
    pub restricted: bool,
}

/// The shelves the running task `job` may use. The task's own conversation,
/// agent and requester decide; nothing the model sends does.
pub(crate) async fn access(db: &impl ConnectionTrait, job: &Job) -> Result<Access> {
    let w = &job.workspace;
    let row = query(
        db,
        "SELECT c.kind,c.visibility='public' AS public,c.work,
           (SELECT count(*) FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id) AS people,
           COALESCE((SELECT work_class FROM relay_chat_jobs j WHERE j.workspace=c.workspace AND j.reply=$3),'interactive') AS class,
           EXISTS(SELECT 1 FROM relay_assistants a WHERE a.workspace=c.workspace AND a.agent=$4) AS assistant,
           EXISTS(SELECT 1 FROM relay_context_attempts t WHERE t.workspace=c.workspace AND t.task=$3
             AND jsonb_array_length(COALESCE(t.prepared->'sources','[]'::jsonb))>0) AS restricted,
           CASE WHEN to_regclass('relay_communication_inbox') IS NULL THEN false ELSE
             EXISTS(SELECT 1 FROM relay_communication_inbox i WHERE i.workspace=c.workspace AND i.chat=c.id AND i.transport<>'chat') END AS outside
         FROM relay_chats c WHERE c.workspace=$1 AND c.id=$2",
        vec![
            w.clone().into(),
            job.chat.clone().into(),
            job.reply.clone().into(),
            job.agent.clone().into(),
        ],
    )
    .await?
    .context("Conversation missing")?;
    let kind: String = row.try_get("", "kind")?;
    let public: bool = row.try_get("", "public")?;
    let work: bool = row.try_get("", "work")?;
    let people: i64 = row.try_get("", "people")?;
    let background = row.try_get::<String>("", "class")? == "background";
    let assistant: bool = row.try_get("", "assistant")?;
    let restricted: bool = row.try_get("", "restricted")?;
    let outside: bool = row.try_get("", "outside")?;
    if assistant {
        return Ok(Access {
            assistant,
            read: vec![Shelf::workspace()],
            here: None,
            open: false,
            waits_because: Some("private_source"),
            restricted,
        });
    }
    let agent = &job.agent;
    let shared = kind != "direct" && (public || people > 1);
    let (read, here) = if outside {
        // People outside the workspace read the reply: only this thread's own notes.
        let room = Shelf::room(agent, &job.chat);
        (vec![room.clone()], room)
    } else if shared || work {
        let room = Shelf::room(agent, &job.chat);
        (
            vec![room.clone(), Shelf::job(agent), Shelf::workspace()],
            room,
        )
    } else {
        let person = Shelf::person(agent, &job.requester);
        (
            vec![person.clone(), Shelf::job(agent), Shelf::workspace()],
            person,
        )
    };
    let waits_because = if outside {
        Some("outside")
    } else if background || work {
        Some("background")
    } else if !public || kind == "direct" || restricted {
        Some("private_source")
    } else {
        None
    };
    Ok(Access {
        assistant,
        read,
        here: Some(here),
        open: waits_because.is_none(),
        waits_because,
        restricted,
    })
}

/// One line, without line breaks or outer spaces, cut to what a row holds.
pub(crate) fn line(text: &str, most: usize) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(most)
        .collect()
}

/// The name an agent uses for a note: the start of its id.
pub(crate) fn name(id: &str) -> &str {
    id.get(..8).unwrap_or(id)
}

/// The words of a query for a prefix text search, any of them matching.
pub(crate) fn any_words(text: &str, most: usize) -> Option<String> {
    let words: Vec<String> = text
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| word.chars().count() > 1)
        .take(most)
        .map(|word| format!("{}:*", word.to_lowercase()))
        .collect();
    (!words.is_empty()).then(|| words.join(" | "))
}

pub(crate) use migration::Memory;
mod migration {
    use sea_orm_migration::prelude::*;
    pub(crate) struct Memory;
    impl MigrationName for Memory {
        fn name(&self) -> &str {
            "m20261124_000142_memory"
        }
    }
    #[async_trait::async_trait]
    impl MigrationTrait for Memory {
        async fn up(&self, m: &SchemaManager) -> Result<(), DbErr> {
            m.get_connection()
                .execute_unprepared(include_str!("memory.sql"))
                .await?;
            Ok(())
        }
        async fn down(&self, _: &SchemaManager) -> Result<(), DbErr> {
            Err(DbErr::Custom("Memory requires a forward migration".into()))
        }
    }
}

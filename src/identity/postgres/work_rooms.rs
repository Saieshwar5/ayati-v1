//! Work rooms (schema 56). A schedule runs in its own private room with the
//! person who set it up and the agent, never in a conversation people use. The
//! room never appears in Chat; its runs show in Activity, and it is the
//! schedule's memory of earlier runs. A schedule may also post one short result
//! message to a conversation, if its person chose one.
use super::automation_access::authority;
use super::*;
use crate::identity::chat::model::Job;
#[path = "work_rooms_migration.rs"]
pub(super) mod migration;
#[cfg(test)]
#[path = "work_rooms_tests.rs"]
mod tests;

/// Characters of the result shown in a posted message; the rest is in Activity.
const POSTED_CHARS: usize = 400;

/// The schedule's own room, opened at its first run.
pub(super) async fn room(tx: &DatabaseTransaction, w: &str, schedule: &Value) -> Result<String> {
    if let Some(room) = schedule["room"].as_str() {
        return Ok(room.to_owned());
    }
    let (id, agent, person) = (
        schedule["id"].as_str().context("Missing schedule")?,
        schedule["agent"].as_str().context("Missing agent")?,
        schedule["principal"].as_str().context("Missing person")?,
    );
    let room = uuid::Uuid::new_v4().to_string();
    execute(tx, "INSERT INTO relay_chats(id,workspace,name,creator,kind,description,work) VALUES($1,$2,$3,$4,'direct','Runs of a schedule',TRUE)",
        vec![room.clone().into(), w.into(), schedule["name"].as_str().unwrap_or("Schedule").into(), person.into()]).await?;
    execute(
        tx,
        "INSERT INTO relay_chat_people(workspace,chat,user_id) VALUES($1,$2,$3)",
        vec![w.into(), room.clone().into(), person.into()],
    )
    .await?;
    execute(
        tx,
        "INSERT INTO relay_chat_agents(workspace,chat,agent,joined_sequence) VALUES($1,$2,$3,0)",
        vec![w.into(), room.clone().into(), agent.into()],
    )
    .await?;
    execute(
        tx,
        "UPDATE relay_agent_schedules SET room=$3 WHERE workspace=$1 AND id=$2 AND room IS NULL",
        vec![w.into(), id.into(), room.clone().into()],
    )
    .await?;
    Ok(room)
}

/// Whether this schema has work rooms (56+), so current code runs on older ones.
pub(super) async fn ready(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regclass('relay_run_posts') IS NOT NULL AS ready",
        vec![],
    )
    .await?
    .context("Missing schema check")?
    .try_get("", "ready")?)
}

/// ` AND NOT c.work` where work rooms exist, for queries listing conversations.
pub(super) async fn hidden(db: &impl ConnectionTrait) -> Result<&'static str> {
    Ok(if ready(db).await? {
        " AND NOT c.work"
    } else {
        ""
    })
}

pub(super) async fn is_room(db: &impl ConnectionTrait, w: &str, chat: &str) -> Result<bool> {
    if !ready(db).await? {
        return Ok(false);
    }
    Ok(query(
        db,
        "SELECT EXISTS(SELECT 1 FROM relay_chats WHERE workspace=$1 AND id=$2 AND work) AS work",
        vec![w.into(), chat.into()],
    )
    .await?
    .context("Conversation missing")?
    .try_get("", "work")?)
}

/// People don't write into a work room: its runs come from the schedule.
pub(super) async fn ordinary(db: &impl ConnectionTrait, w: &str, chat: &str) -> Result<()> {
    ensure!(
        !is_room(db, w, chat).await?,
        "This room holds a schedule's runs. Open them in Activity."
    );
    Ok(())
}

/// After a scheduled run completes, posts one short message with its result
/// where the schedule says, if the person who set it up may still post there.
/// Either way the outcome is recorded for Activity.
pub(super) async fn post_result(tx: &DatabaseTransaction, job: &Job, content: &str) -> Result<()> {
    if !is_room(tx, &job.workspace, &job.chat).await? {
        return Ok(());
    }
    let Some(schedule) = json_rows(tx, "SELECT jsonb_build_object('name',s.name,'chat',s.chat,'principal',s.principal) AS data FROM relay_schedule_occurrences o
        JOIN relay_agent_schedules s ON s.workspace=o.workspace AND s.id=o.schedule WHERE o.workspace=$1 AND o.reply=$2 AND s.chat IS NOT NULL",
        vec![job.workspace.clone().into(), job.reply.clone().into()]).await?.into_iter().next() else {
        return Ok(());
    };
    let target = schedule["chat"].as_str().context("Missing post target")?;
    let person = User {
        id: schedule["principal"]
            .as_str()
            .context("Missing person")?
            .into(),
        email: String::new(),
        display_name: None,
    };
    let w = &job.workspace;
    let (state, message, reason) = match authority(tx, &person, w, &job.agent, target).await {
        Ok(()) => {
            let mut shown: String = content.chars().take(POSTED_CHARS).collect();
            if content.chars().count() > POSTED_CHARS {
                shown.push('…');
            }
            let id = uuid::Uuid::new_v4().to_string();
            let text = format!(
                "**{}**\n\n{shown}\n\n[View run](run:{})",
                schedule["name"].as_str().unwrap_or("Schedule"),
                job.reply
            );
            execute(tx, "INSERT INTO relay_chat_messages(id,workspace,chat,author,kind,content,state,at) VALUES($1,$2,$3,$4,'agent',$5,'complete',$6)",
                vec![id.clone().into(), w.into(), target.into(), job.agent.clone().into(), text.into(), now().into()]).await?;
            ("posted", Some(id), None)
        }
        Err(error) if error.is::<sea_orm::DbErr>() => return Err(error),
        Err(_) => (
            "skipped",
            None,
            Some("The person who set up the schedule can no longer post there with this agent."),
        ),
    };
    execute(tx, "INSERT INTO relay_run_posts(workspace,reply,chat,message,state,reason,created) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING",
        vec![w.into(), job.reply.clone().into(), target.into(), message.into(), state.into(), reason.into(), now().into()]).await?;
    Ok(())
}

/// What Activity shows about a run that came from a schedule.
pub(super) async fn run_source(
    db: &impl ConnectionTrait,
    w: &str,
    reply: &str,
) -> Result<Option<Value>> {
    if !ready(db).await? {
        return Ok(None);
    }
    Ok(json_rows(db, "SELECT jsonb_build_object('id',s.id,'name',s.name,'agent',s.agent,'state',s.state,'revision',s.revision,
            'principal',s.principal,'interval_seconds',s.interval_seconds,'post_to',c.name,
            'posted',p.state,'post_reason',p.reason,
            'room',EXISTS(SELECT 1 FROM relay_chat_messages m JOIN relay_chats r ON r.workspace=m.workspace AND r.id=m.chat WHERE m.workspace=o.workspace AND m.id=o.reply AND r.work)) AS data
        FROM relay_schedule_occurrences o JOIN relay_agent_schedules s ON s.workspace=o.workspace AND s.id=o.schedule
        LEFT JOIN relay_chats c ON c.workspace=s.workspace AND c.id=s.chat
        LEFT JOIN relay_run_posts p ON p.workspace=o.workspace AND p.reply=o.reply
        WHERE o.workspace=$1 AND o.reply=$2", vec![w.into(), reply.into()]).await?.into_iter().next())
}

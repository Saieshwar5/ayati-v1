//! Work rooms (schema 56). A task's or trigger's runs happen in its own private
//! room with the person it is for and the agent, never in a conversation people
//! use. The room never appears in Chat; its runs show in Activity, and it is the
//! memory of earlier runs. Each run may post one short result message to a
//! conversation (the owner's channel rule, schema 145). Schedules worked this way
//! until schema 147 made them tasks.
use super::automation_access::authority_for;
use super::*;
use crate::identity::chat::model::Job;
#[path = "work_rooms_migration.rs"]
pub(super) mod migration;
#[cfg(test)]
#[path = "work_rooms_tests.rs"]
mod tests;

/// Characters of the result shown in a posted message; the rest is in Activity.
const POSTED_CHARS: usize = 400;

/// Opens a new work room holding `person` and `agent`. Work rooms never appear
/// in Chat, and people can't write into them.
pub(super) async fn open(
    tx: &DatabaseTransaction,
    w: &str,
    name: &str,
    description: &str,
    person: &str,
    agent: &str,
) -> Result<String> {
    let room = uuid::Uuid::new_v4().to_string();
    execute(tx, "INSERT INTO relay_chats(id,workspace,name,creator,kind,description,work) VALUES($1,$2,$3,$4,'direct',$5,TRUE)",
        vec![room.clone().into(), w.into(), name.into(), person.into(), description.into()]).await?;
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

/// People don't write into a work room: its runs come from a task or trigger.
pub(super) async fn ordinary(db: &impl ConnectionTrait, w: &str, chat: &str) -> Result<()> {
    ensure!(
        !is_room(db, w, chat).await?,
        "This room holds an automation's runs. Open them in Activity."
    );
    Ok(())
}

/// After an event trigger's run (or a schedule's run that started before schema
/// 147) completes, posts one short message with its result where it says, if the
/// person who set it up may still post there. Either way the outcome is recorded
/// for Activity. Task runs post from `tasks::runs::after_finish`.
pub(super) async fn post_result(tx: &DatabaseTransaction, job: &Job, content: &str) -> Result<()> {
    if !is_room(tx, &job.workspace, &job.chat).await? {
        return Ok(());
    }
    let mut found = json_rows(tx, "SELECT jsonb_build_object('name',s.name,'chat',s.chat,'principal',s.principal,'kind','schedule') AS data FROM relay_schedule_occurrences o
        JOIN relay_agent_schedules s ON s.workspace=o.workspace AND s.id=o.schedule WHERE o.workspace=$1 AND o.reply=$2 AND s.chat IS NOT NULL",
        vec![job.workspace.clone().into(), job.reply.clone().into()]).await?.into_iter().next();
    if found.is_none() && super::automation_memory::ready(tx).await? {
        found = json_rows(tx, "SELECT jsonb_build_object('name',t.input->>'name','chat',t.chat,'principal',t.principal,'kind','event') AS data FROM relay_event_deliveries d
            JOIN relay_event_triggers t ON t.workspace=d.workspace AND t.id=d.trigger_id WHERE d.workspace=$1 AND d.reply=$2 AND d.room IS NOT NULL AND t.chat IS NOT NULL AND NOT d.revoked",
            vec![job.workspace.clone().into(), job.reply.clone().into()]).await?.into_iter().next();
    }
    let Some(source) = found else {
        return Ok(());
    };
    let target = source["chat"].as_str().context("Missing post target")?;
    post(
        tx,
        job,
        content,
        source["name"].as_str().unwrap_or("Automation"),
        source["principal"].as_str().context("Missing person")?,
        target,
        source["kind"] == "event",
    )
    .await
}

/// Post one short message with a run's result in `target` for `principal`, if
/// they may still post there with this agent; either way record the outcome.
/// Used by triggers (`manage`: their person must still look after the agent) and
/// by tasks a channel asked for (schema 145; their person must still be allowed
/// to ask the agent).
pub(super) async fn post(
    tx: &DatabaseTransaction,
    job: &Job,
    content: &str,
    name: &str,
    principal: &str,
    target: &str,
    manage: bool,
) -> Result<()> {
    let person = User {
        id: principal.into(),
        email: String::new(),
        display_name: None,
    };
    let w = &job.workspace;
    let (state, message, reason) = match authority_for(tx, &person, w, &job.agent, manage, target)
        .await
    {
        Ok(()) => {
            let mut shown: String = content.chars().take(POSTED_CHARS).collect();
            if content.chars().count() > POSTED_CHARS {
                shown.push('…');
            }
            let id = uuid::Uuid::new_v4().to_string();
            let text = format!("**{name}**\n\n{shown}\n\n[View run](run:{})", job.reply);
            execute(tx, "INSERT INTO relay_chat_messages(id,workspace,chat,author,kind,content,state,at) VALUES($1,$2,$3,$4,'agent',$5,'complete',$6)",
                vec![id.clone().into(), w.into(), target.into(), job.agent.clone().into(), text.into(), now().into()]).await?;
            ("posted", Some(id), None)
        }
        Err(error) if error.is::<sea_orm::DbErr>() => return Err(error),
        Err(_) => (
            "skipped",
            None,
            Some("The person who set up the automation can no longer post there with this agent."),
        ),
    };
    execute(tx, "INSERT INTO relay_run_posts(workspace,reply,chat,message,state,reason,created) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT DO NOTHING",
        vec![w.into(), job.reply.clone().into(), target.into(), message.into(), state.into(), reason.into(), now().into()]).await?;
    Ok(())
}

/// A posted run summary carries the run's result, so it follows the run's own
/// source checks: when the viewer may no longer see the run (an event source's
/// read was revoked, a Library document was hidden), the summary is hidden too.
pub(super) async fn guard_posts(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    messages: &mut [Value],
) -> Result<()> {
    if !ready(tx).await? {
        return Ok(());
    }
    let ids = messages
        .iter()
        .filter(|m| m["kind"] == "agent")
        .map(|m| m["id"].clone())
        .collect::<Vec<_>>();
    let posts = rows(tx, "SELECT p.message,j.payload FROM relay_run_posts p JOIN relay_chat_jobs j ON j.workspace=p.workspace AND j.reply=p.reply
        WHERE p.workspace=$1 AND p.message IN (SELECT jsonb_array_elements_text($2::jsonb))",
        vec![w.into(), serde_json::json!(ids).into()]).await?;
    for row in posts {
        let message: String = row.try_get("", "message")?;
        let mut viewer: Job = serde_json::from_value(row.try_get("", "payload")?)?;
        viewer.requester = user.id.clone();
        if super::source_access::allowed(tx, &viewer).await? {
            continue;
        }
        for shown in messages.iter_mut().filter(|m| m["id"] == message.as_str()) {
            shown["content"] =
                Value::String("Result unavailable because source access changed.".into());
            shown["response"] = Value::Null;
            shown["attachments"] = serde_json::json!([]);
            shown["response_unavailable"] = Value::Bool(true);
        }
    }
    Ok(())
}

/// What Activity shows about a run that came from a schedule (before schema 147),
/// with the task it became (`task`), whose page has its later runs.
pub(super) async fn run_source(
    db: &impl ConnectionTrait,
    w: &str,
    reply: &str,
) -> Result<Option<Value>> {
    if !ready(db).await? {
        return Ok(None);
    }
    let task = if super::tasks::wakeups::rooms(db).await? {
        "(SELECT t.id FROM relay_tasks t WHERE t.workspace=s.workspace AND t.schedule=s.id LIMIT 1)"
    } else {
        "NULL::text"
    };
    Ok(json_rows(db, &format!("SELECT jsonb_build_object('id',s.id,'name',s.name,'agent',s.agent,'state',s.state,'revision',s.revision,
            'principal',s.principal,'interval_seconds',s.interval_seconds,'post_to',c.name,
            'posted',p.state,'post_reason',p.reason,'task',{task},
            'room',EXISTS(SELECT 1 FROM relay_chat_messages m JOIN relay_chats r ON r.workspace=m.workspace AND r.id=m.chat WHERE m.workspace=o.workspace AND m.id=o.reply AND r.work)) AS data
        FROM relay_schedule_occurrences o JOIN relay_agent_schedules s ON s.workspace=o.workspace AND s.id=o.schedule
        LEFT JOIN relay_chats c ON c.workspace=s.workspace AND c.id=s.chat
        LEFT JOIN relay_run_posts p ON p.workspace=o.workspace AND p.reply=o.reply
        WHERE o.workspace=$1 AND o.reply=$2"), vec![w.into(), reply.into()]).await?.into_iter().next())
}

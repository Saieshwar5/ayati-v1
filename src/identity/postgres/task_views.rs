//! Tasks for people: Activity's Open list, one task's details, closing a task, and
//! "Make this a task" on a request. Visibility follows the task (`super::VISIBLE`):
//! the person it is for, anyone who can see its conversation, the agent's managers
//! and admins. My agents narrows it to tasks for the person and agents they manage.
use super::super::activity::{Filters, query_with, valid_scope};
use super::super::*;
use super::{VISIBLE, line};
use serde_json::json;

/// Open first by what needs a person, then by age.
const ORDER: &str = "CASE s.status WHEN 'needs_fixing' THEN 0 WHEN 'late' THEN 1 WHEN 'waiting' THEN 2 WHEN 'working' THEN 3 WHEN 'open' THEN 4 ELSE 5 END";

/// What a waiting task waits for.
const WAITING_ON: &str = "CASE WHEN EXISTS(SELECT 1 FROM relay_chat_jobs j JOIN relay_control_actions x ON x.workspace=j.workspace AND x.job=j.reply
   WHERE j.workspace=t.workspace AND j.task=t.id AND j.state='waiting' AND x.state='pending' AND x.requires_approval) THEN 'approval'
  WHEN EXISTS(SELECT 1 FROM relay_chat_jobs j JOIN relay_task_questions q ON q.workspace=j.workspace AND q.reply=j.reply
   WHERE j.workspace=t.workspace AND j.task=t.id AND j.state='waiting' AND q.state='pending') THEN 'question'
  WHEN EXISTS(SELECT 1 FROM relay_chat_jobs j JOIN relay_agent_handoffs h ON h.workspace=j.workspace AND h.parent=j.reply
   WHERE j.workspace=t.workspace AND j.task=t.id AND j.state='waiting' AND h.state='pending') THEN 'helper'
  ELSE NULL END";

/// One row's fields, for the Open list and details.
const FIELDS: &str = "'id',t.id,'title',t.title,'kind',t.kind,'origin',t.origin,'done_when',t.done_when,'status',s.status,
 'agent',a.id,'agent_name',a.name,'agent_avatar',a.settings->>'avatar','requester',t.requester,
 'requester_name',(SELECT COALESCE(u.display_name,u.email) FROM relay_users u WHERE u.id=t.requester),
 'chat',t.chat,'conversation_name',c.name,'conversation_kind',c.kind,'created_ms',t.created_ms,'start_ms',t.start_ms,'due_ms',t.due_ms,
 'closed_ms',t.closed_ms,'outcome',t.outcome,'result',t.result,
 'steps',(SELECT count(*) FROM relay_chat_jobs j WHERE j.workspace=t.workspace AND j.task=t.id)";

async fn admin(tx: &DatabaseTransaction, w: &str, user: &User) -> Result<bool> {
    Ok(role(tx, w, &user.id).await? == "Admin")
}

impl PgStore {
    /// Activity → Open: every open task the person may see, what needs a person first.
    pub(crate) async fn tasks_open(
        &self,
        user: &User,
        w: &str,
        scope: Option<&str>,
    ) -> Result<Value> {
        valid_scope(scope)?;
        let tx = self.activity_tx(w, false).await?;
        let admin = admin(&tx, w, user).await?;
        let sql = format!("SELECT jsonb_build_object({FIELDS},'waiting_on',{WAITING_ON}) AS data
            FROM relay_tasks t CROSS JOIN LATERAL (SELECT relay_task_status(t.workspace,t.id) AS status) s
            JOIN relay_shared_agents a ON a.workspace=t.workspace AND a.id=t.agent
            JOIN relay_chats c ON c.workspace=t.workspace AND c.id=t.chat
            WHERE t.workspace=$1 AND t.outcome IS NULL AND {VISIBLE}
             AND s.status NOT IN ('done','failed','cancelled')
             AND ($4::text IS DISTINCT FROM 'mine' OR t.requester=$2
              OR EXISTS(SELECT 1 FROM relay_agent_managers g WHERE g.workspace=t.workspace AND g.agent=t.agent AND g.user_id=$2))
            ORDER BY {ORDER},t.created_ms DESC LIMIT 201");
        let mut items = json_rows(
            &tx,
            &sql,
            vec![
                w.into(),
                user.id.clone().into(),
                admin.into(),
                scope.map(str::to_string).into(),
            ],
        )
        .await?;
        tx.commit().await?;
        let more = items.len() > 200;
        items.truncate(200);
        Ok(json!({"items": items, "more": more}))
    }

    /// One task: its fields, its requests in order, and what the person may do.
    pub(crate) async fn task_details(&self, user: &User, w: &str, id: &str) -> Result<Value> {
        let tx = self.activity_tx(w, false).await?;
        let admin = admin(&tx, w, user).await?;
        let sql = format!("SELECT jsonb_build_object({FIELDS},'waiting_on',{WAITING_ON},
             'may_close',(t.requester=$2 OR $3 OR EXISTS(SELECT 1 FROM relay_agent_managers g WHERE g.workspace=t.workspace AND g.agent=t.agent AND g.user_id=$2))) AS data
            FROM relay_tasks t CROSS JOIN LATERAL (SELECT relay_task_status(t.workspace,t.id) AS status) s
            JOIN relay_shared_agents a ON a.workspace=t.workspace AND a.id=t.agent
            JOIN relay_chats c ON c.workspace=t.workspace AND c.id=t.chat
            WHERE t.workspace=$1 AND t.id=$4 AND {VISIBLE}");
        let mut task: Value = query(
            &tx,
            &sql,
            vec![w.into(), user.id.clone().into(), admin.into(), id.into()],
        )
        .await?
        .context("Task unavailable")?
        .try_get("", "data")?;
        // Each step is a request; the person opens its details in Activity, which
        // applies the request's own visibility.
        task["requests"] = json!(json_rows(&tx, "SELECT jsonb_build_object('id',j.reply,'state',m.state,'created_ms',j.created_ms,
             'summary',COALESCE(left((SELECT r.content FROM relay_chat_messages r WHERE r.workspace=j.workspace AND r.id=j.request_message),180),'Request')) AS data
            FROM relay_chat_jobs j JOIN relay_chat_messages m ON m.workspace=j.workspace AND m.id=j.reply
            WHERE j.workspace=$1 AND j.task=$2 ORDER BY j.sequence LIMIT 100", vec![w.into(), id.into()]).await?);
        tx.commit().await?;
        Ok(task)
    }

    /// A person closes a task: done or cancelled (the person it is for, the agent's
    /// managers, admins), or "Not a task" (anyone who can see it).
    pub(crate) async fn task_close(
        &self,
        user: &User,
        w: &str,
        id: &str,
        outcome: &str,
        result: Option<&str>,
    ) -> Result<Value> {
        ensure!(
            ["done", "cancelled", "not_task"].contains(&outcome),
            "Choose done, cancelled or not a task"
        );
        let result = result.map(|r| line(r, 1000, "the result")).transpose()?;
        let tx = self.workspace_tx(w).await?;
        let admin = admin(&tx, w, user).await?;
        let sql = format!("SELECT t.outcome,(t.requester=$2 OR $3 OR EXISTS(SELECT 1 FROM relay_agent_managers g WHERE g.workspace=t.workspace AND g.agent=t.agent AND g.user_id=$2)) AS may_close
            FROM relay_tasks t WHERE t.workspace=$1 AND t.id=$4 AND {VISIBLE} FOR UPDATE");
        let row = query(
            &tx,
            &sql,
            vec![w.into(), user.id.clone().into(), admin.into(), id.into()],
        )
        .await?
        .context("Task unavailable")?;
        let current: Option<String> = row.try_get("", "outcome")?;
        if current.as_deref() == Some(outcome) {
            tx.commit().await?;
            return Ok(json!({"id": id, "outcome": outcome}));
        }
        ensure!(current.is_none(), "That task is already closed");
        ensure!(
            outcome == "not_task" || row.try_get::<bool>("", "may_close")?,
            "Only the person it is for, the agent's managers or an admin can close this task"
        );
        execute(&tx, "UPDATE relay_tasks SET outcome=$3,result=$4,closed_by=$5,closed_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE workspace=$1 AND id=$2",
            vec![w.into(), id.into(), outcome.into(), result.into(), user.id.clone().into()]).await?;
        let summary = format!(
            "{} marked a task {}",
            super::super::member_handover::name(&tx, &user.id).await?,
            if outcome == "not_task" {
                "as not a task".to_string()
            } else {
                outcome.to_string()
            }
        );
        audit_with_summary(&tx, &user.id, Some(w), "task_closed", id, &summary).await?;
        tx.commit().await?;
        Ok(json!({"id": id, "outcome": outcome}))
    }

    /// "Make this a task" on a request the person can see: it becomes a commitment
    /// for the person who asked, kept open until someone closes it.
    pub(crate) async fn task_from_request(
        &self,
        user: &User,
        w: &str,
        reply: &str,
        title: Option<&str>,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        let admin = admin(&tx, w, user).await?;
        let mut values = Filters::default().values_for(w, &user.id, admin);
        values.push(reply.into());
        let row = query(&tx, &query_with(include_str!("activity_feed.sql")), values)
            .await?
            .context("Request unavailable")?;
        let summary: Option<String> = row.try_get("", "summary")?;
        let title = match title {
            Some(t) => line(t, 160, "the title")?,
            None => line(
                &summary.unwrap_or_else(|| "Request".into()),
                160,
                "the title",
            )
            .or_else(|_| Ok::<_, anyhow::Error>("Request".into()))?,
        };
        let job = query(&tx, "SELECT j.task,t.kind,j.payload FROM relay_chat_jobs j LEFT JOIN relay_tasks t ON t.workspace=j.workspace AND t.id=j.task WHERE j.workspace=$1 AND j.reply=$2 FOR UPDATE OF j",
            vec![w.into(), reply.into()]).await?.context("Request unavailable")?;
        let existing: Option<String> = job.try_get("", "task")?;
        let id = match (existing, job.try_get::<Option<String>>("", "kind")?) {
            (Some(id), Some(kind)) if kind == "commitment" => id,
            (Some(id), _) => {
                execute(&tx, "UPDATE relay_tasks SET kind='commitment',origin='person',title=$3,created_by=$4 WHERE workspace=$1 AND id=$2",
                    vec![w.into(), id.clone().into(), title.clone().into(), user.id.clone().into()]).await?;
                id
            }
            (None, _) => {
                let payload: Value = job.try_get("", "payload")?;
                let id = uuid::Uuid::new_v4().to_string();
                execute(&tx, "INSERT INTO relay_tasks(workspace,id,agent,kind,origin,title,requester,chat,created_by) VALUES($1,$2,$3,'commitment','person',$4,$5,$6,$7)",
                    vec![w.into(), id.clone().into(), payload["agent"].as_str().unwrap_or_default().into(), title.clone().into(),
                        payload["requester"].as_str().unwrap_or_default().into(), payload["chat"].as_str().unwrap_or_default().into(), user.id.clone().into()]).await?;
                execute(
                    &tx,
                    "UPDATE relay_chat_jobs SET task=$3 WHERE workspace=$1 AND reply=$2",
                    vec![w.into(), reply.into(), id.clone().into()],
                )
                .await?;
                id
            }
        };
        tx.commit().await?;
        Ok(json!({"id": id, "title": title}))
    }
}

/// The task chip on agent replies in Chat: each reply's task, with its status.
pub(in crate::identity::postgres) async fn hydrate(
    tx: &DatabaseTransaction,
    w: &str,
    messages: &mut [Value],
) -> Result<()> {
    let ids: Vec<Value> = messages
        .iter()
        .filter(|m| m["kind"] == "agent")
        .map(|m| m["id"].clone())
        .collect();
    if ids.is_empty() || !super::available(tx).await? {
        return Ok(());
    }
    let rows = rows(tx, "SELECT j.reply,jsonb_build_object('id',t.id,'title',t.title,'kind',t.kind,'status',relay_task_status(t.workspace,t.id)) AS task
        FROM relay_chat_jobs j JOIN relay_tasks t ON t.workspace=j.workspace AND t.id=j.task
        WHERE j.workspace=$1 AND j.reply IN (SELECT jsonb_array_elements_text($2::jsonb)) AND t.outcome IS DISTINCT FROM 'not_task'",
        vec![w.into(), json!(ids).into()]).await?;
    for row in rows {
        let reply: String = row.try_get("", "reply")?;
        if let Some(m) = messages.iter_mut().find(|m| m["id"] == reply.as_str()) {
            m["task"] = row.try_get("", "task")?;
        }
    }
    Ok(())
}

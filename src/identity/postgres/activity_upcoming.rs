//! Activity › Calendar › List (designs/automations-simplified 3): triggers that are
//! always on, and the booked runs, follow-ups and due dates of open tasks. It is
//! seen by the people who look after the agents (as Needs fixing and Coming up) and
//! by whoever set a trigger up; one that posts in a conversation is shown only to
//! people in it. Schedules became tasks in schema 147 and show as tasks here.
use super::fixing::{FIXERS, NOW, exists};
use super::*;
use serde_json::json;

/// A trigger posting in `chat` shows only to its person and people in it.
const POSTS_VISIBLY: &str = "(x.principal=$2 OR EXISTS(SELECT 1 FROM relay_chats c WHERE c.workspace=x.workspace AND c.id=x.chat
  AND (c.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id=$2))))";

impl PgStore {
    pub(crate) async fn activity_upcoming(
        &self,
        user: &User,
        w: &str,
        scope: Option<&str>,
    ) -> Result<Value> {
        super::super::valid_scope(scope)?;
        let tx = self.read_tx(w).await?;
        let admin = role(&tx, w, &user.id).await? == "Admin";
        let args = || -> Vec<sea_orm::Value> {
            vec![
                w.into(),
                user.id.clone().into(),
                admin.into(),
                scope.map(str::to_string).into(),
            ]
        };
        let mine = "(x.agent IN (SELECT id FROM f) OR x.principal=$2)";
        let mut items = json_rows(&tx, &format!("{FIXERS} SELECT jsonb_build_object('kind','trigger','id',x.id,'agent',x.agent,'agent_name',a.name,
              'title',COALESCE(x.input->>'name',x.source_definition->>'name'),'source',x.source_definition->>'name','state',x.state,'revision',x.revision,
              'detail',x.detail,'by',COALESCE(u.display_name,u.email),'own',x.principal=$2) AS data
            FROM relay_event_triggers x JOIN relay_shared_agents a ON a.workspace=x.workspace AND a.id=x.agent JOIN relay_users u ON u.id=x.principal
            WHERE x.workspace=$1 AND x.state IN ('enabled','paused','blocked') AND {mine} AND {POSTS_VISIBLY}
            ORDER BY a.name,x.created_ms LIMIT 100"), args()).await?;
        if exists(&tx, "relay_task_wakeups").await? {
            // Follow-ups the agents booked, and open tasks due within a week.
            let week = format!("BETWEEN {NOW} AND {NOW}+604800000");
            items.extend(json_rows(&tx, &format!("{FIXERS} SELECT jsonb_build_object('kind',CASE WHEN k.id IS NULL THEN 'due' ELSE 'follow_up' END,'id',t.id,
                  'agent',t.agent,'agent_name',a.name,'title',t.title,'at',COALESCE(k.due_ms,t.due_ms)) AS data
                FROM relay_tasks t JOIN relay_shared_agents a ON a.workspace=t.workspace AND a.id=t.agent
                LEFT JOIN relay_task_wakeups k ON k.workspace=t.workspace AND k.task=t.id AND k.state='pending'
                WHERE t.workspace=$1 AND t.outcome IS NULL AND (k.id IS NOT NULL OR t.due_ms {week})
                  AND (t.requester=$2 OR t.agent IN (SELECT id FROM f))
                ORDER BY 1 LIMIT 100"), args()).await?);
        }
        tx.commit().await?;
        Ok(json!({"items": items}))
    }
}

#[cfg(test)]
#[path = "activity_upcoming_tests.rs"]
mod tests;

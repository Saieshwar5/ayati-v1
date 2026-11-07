use super::*;
use crate::identity::chat::{model::Job, organization_model::Lifecycle};
use serde_json::json;
pub(super) async fn accepting(tx: &impl ConnectionTrait, w: &str, chat: &str) -> Result<()> {
    let row = query(
        tx,
        "SELECT archived FROM relay_chats WHERE workspace=$1 AND id=$2",
        vec![w.into(), chat.into()],
    )
    .await?
    .context("Conversation unavailable")?;
    ensure!(
        !row.try_get::<bool>("", "archived")?,
        "This channel is archived. Restore it before sending new messages."
    );
    Ok(())
}
impl PgStore {
    pub(crate) async fn check_chat_accepting(
        &self,
        user: &User,
        w: &str,
        chat: &str,
    ) -> Result<()> {
        let tx = self.read_tx(w).await?;
        super::chat::access(&tx, user, w, chat).await?;
        accepting(&tx, w, chat).await?;
        tx.commit().await?;
        Ok(())
    }
    pub(crate) async fn channel_lifecycle(
        &self,
        user: &User,
        w: &str,
        chat: &str,
        input: Option<&Lifecycle>,
    ) -> Result<Value> {
        if let Some(input) = input {
            input.validate()?;
        }
        let tx = if input.is_some() {
            self.workspace_tx(w).await?
        } else {
            self.read_tx(w).await?
        };
        super::chat::access(&tx, user, w, chat).await?;
        let row=query(&tx,"SELECT kind,creator,archived,archive_revision,archive_request,archive_payload FROM relay_chats WHERE workspace=$1 AND id=$2",vec![w.into(),chat.into()]).await?.context("Conversation unavailable")?;
        let manageable = row.try_get::<String>("", "kind")? != "direct"
            && (role(&tx, w, &user.id).await? == "Admin"
                || row.try_get::<String>("", "creator")? == user.id);
        let mut archived: bool = row.try_get("", "archived")?;
        let mut revision: i64 = row.try_get("", "archive_revision")?;
        if let Some(input) = input {
            ensure!(
                manageable,
                "Only a channel creator or workspace admin with source access can archive or restore it"
            );
            let payload = serde_json::to_value(input)?;
            if row
                .try_get::<Option<String>>("", "archive_request")?
                .as_deref()
                == Some(&input.request)
            {
                ensure!(
                    row.try_get::<Option<Value>>("", "archive_payload")? == Some(payload),
                    "Channel request payload changed"
                );
            } else {
                ensure!(
                    revision == input.revision,
                    "Channel state changed. Refresh before saving again."
                );
                ensure!(
                    archived != input.archived,
                    "Channel already has this state. Refresh its details."
                );
                execute(&tx,"UPDATE relay_chats SET archived=$3,archive_revision=archive_revision+1,archived_at=CASE WHEN $3 THEN $4 ELSE NULL END,archived_by=CASE WHEN $3 THEN $5 ELSE NULL END,archive_request=$6,archive_payload=$7 WHERE workspace=$1 AND id=$2",vec![w.into(),chat.into(),input.archived.into(),now().into(),user.id.clone().into(),input.request.clone().into(),payload.into()]).await?;
                if input.archived {
                    super::automation_access::invalidate_inaccessible(&tx, w).await?;
                    super::event_dispatch::invalidate_inaccessible(&tx, w).await?;
                }
                archived = input.archived;
                revision += 1;
                audit(
                    &tx,
                    &user.id,
                    Some(w),
                    if archived {
                        "channel_archived"
                    } else {
                        "channel_restored"
                    },
                    chat,
                )
                .await?;
            }
        }
        // Everyone with access sees how many visible requests are unfinished;
        // people who may archive also see the first ones, to open or stop them.
        let (mut active, mut unfinished) = (0, vec![]);
        let sql = UNFINISHED.replace("/*SOURCE*/", include_str!("activity_source.sql"));
        for row in rows(&tx, &sql, vec![w.into(), chat.into()]).await? {
            let mut job: Job = serde_json::from_value(row.try_get("", "payload")?)?;
            job.requester = user.id.clone();
            if !super::source_access::allowed(&tx, &job).await? {
                continue;
            }
            active += 1;
            if manageable && unfinished.len() < SHOWN_UNFINISHED {
                unfinished.push(json!({
                    "request": job.reply, "agent": job.agent,
                    "agent_name": row.try_get::<Option<String>>("", "agent_name")?,
                    "agent_avatar": row.try_get::<Option<String>>("", "agent_avatar")?,
                    "summary": row.try_get::<Option<String>>("", "summary")?,
                    "state": row.try_get::<String>("", "state")?,
                    "started_ms": row.try_get::<Option<i64>>("", "started_ms")?,
                }));
            }
        }
        tx.commit().await?;
        let mut result = json!({"archived":archived,"revision":revision,"manageable":manageable,"active_requests":active});
        if manageable {
            result["more"] = json!(active - unfinished.len());
            result["unfinished"] = json!(unfinished);
        }
        Ok(result)
    }
}

/// How many unfinished requests the archive review lists; it counts the rest.
const SHOWN_UNFINISHED: usize = 20;
/// A channel's unfinished requests, oldest first: the asking message (as in
/// Activity), the agent, what a waiting request waits for and when it started.
const UNFINISHED: &str = "SELECT j.payload,COALESCE(j.started_ms,j.created_ms) AS started_ms,
  ag.name AS agent_name,ag.settings->>'avatar' AS agent_avatar,
  (SELECT left(s.content,200) FROM (/*SOURCE*/) s) AS summary,
  CASE WHEN j.state<>'waiting' THEN j.state
   WHEN EXISTS(SELECT 1 FROM relay_control_actions x WHERE x.workspace=j.workspace AND x.job=j.reply AND x.state='pending' AND x.requires_approval) THEN 'approval'
   WHEN EXISTS(SELECT 1 FROM relay_task_questions q WHERE q.workspace=j.workspace AND q.reply=j.reply AND q.state='pending') THEN 'question'
   ELSE 'waiting' END AS state
 FROM relay_chat_jobs j JOIN relay_chat_messages m ON m.workspace=j.workspace AND m.id=j.reply
 LEFT JOIN relay_shared_agents ag ON ag.workspace=j.workspace AND ag.id=j.payload->>'agent'
 WHERE j.workspace=$1 AND j.payload->>'chat'=$2 AND j.state<>'finished'
 ORDER BY j.sequence";

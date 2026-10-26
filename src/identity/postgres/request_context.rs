//! Rebuild eligible evidence at each attempt, retaining the original request.
use super::*;
use crate::identity::chat::store::Job;
use serde_json::json;

impl PgStore {
    #[cfg(test)]
    pub(crate) async fn request_identity(&self, job: &Job) -> Result<Value> {
        let tx = self.read_tx(&job.workspace).await?;
        super::control_actions::authorize(&tx, job, true).await?;
        let identity = identity_in(&tx, job).await?;
        tx.commit().await?;
        Ok(identity)
    }

    pub(crate) async fn request_context(&self, job: &Job) -> Result<(Value, String)> {
        let tx = self.read_tx(&job.workspace).await?;
        super::control_actions::authorize(&tx, job, true).await?;
        let identity = identity_in(&tx, job).await?;
        let request = if let Some(id) = identity["request_message"].as_str() {
            let row = query(&tx,
                "SELECT a.revision,a.settings,m.sequence,r.thread,j.payload->>'prompt' AS stored_prompt,
                    EXISTS(SELECT 1 FROM relay_library_sources s WHERE s.workspace=m.workspace AND s.reply=m.id) AS library_source
                 FROM relay_shared_agents a
                 JOIN relay_chat_messages m ON m.workspace=a.workspace AND m.id=$3
                 JOIN relay_chat_jobs j ON j.workspace=m.workspace AND j.reply=m.id
                 JOIN relay_chat_messages r ON r.workspace=m.workspace AND r.chat=m.chat AND r.id=$4 AND r.sequence<m.sequence
                 WHERE a.workspace=$1 AND a.id=$2",
                vec![job.workspace.clone().into(),job.agent.clone().into(),job.reply.clone().into(),id.into()]
            ).await?.context("Original request evidence unavailable")?;
            let managed = row.try_get::<i64>("", "revision")? > 0;
            let settings: Value = row.try_get("", "settings")?;
            let thread = if managed {
                None
            } else {
                row.try_get::<Option<String>>("", "thread")?.or_else(|| {
                    (identity["conversation"]["kind"] != "direct").then(|| id.to_owned())
                })
            };
            let mut messages = super::conversation_context::recent(
                &tx,
                super::conversation_context::RecentScope {
                    workspace: &job.workspace,
                    chat: &job.chat,
                    agent: &job.agent,
                    thread: thread.as_deref(),
                    since_join: settings["history"] == "since_join",
                    limit: if managed { 60 } else { 30 },
                    before: row.try_get("", "sequence")?,
                    request: id,
                },
            )
            .await?;
            ensure!(
                messages.iter().any(|message| message["id"] == id),
                "Original request is no longer eligible for this task"
            );
            if row.try_get::<bool>("", "library_source")? {
                // The Library's display message appends a generated source link.
                // Keep its canonical question separate from explicit sources.
                let question: String = row.try_get("", "stored_prompt")?;
                if let Some(message) = messages.iter_mut().find(|message| message["id"] == id) {
                    message["content"] = json!(question);
                }
            }
            super::conversation_context::prompt(id, messages)?
        } else {
            // Old migrations and historical Library tasks have no exact request
            // link. Preserve their stored payload, never guess another message.
            job.prompt.clone()
        };
        tx.commit().await?;
        Ok((identity, request))
    }
}

async fn identity_in(tx: &DatabaseTransaction, job: &Job) -> Result<Value> {
    let row = query(tx,
        "SELECT u.id,u.email,u.display_name,c.kind,j.request_message,j.work_class,m.thread,
            original.author AS sender,original.kind AS sender_kind,
            CASE WHEN original.kind='agent' THEN sender_agent.name ELSE sender_user.display_name END AS sender_name
         FROM relay_chat_jobs j
         JOIN relay_chat_messages m ON m.workspace=j.workspace AND m.id=j.reply
         JOIN relay_chats c ON c.workspace=m.workspace AND c.id=m.chat
         JOIN relay_members p ON p.workspace=j.workspace AND p.user_id=$3
         JOIN relay_users u ON u.id=p.user_id
         LEFT JOIN relay_chat_messages original ON original.workspace=j.workspace AND original.chat=m.chat AND original.id=j.request_message
         LEFT JOIN relay_users sender_user ON sender_user.id=original.author AND original.kind='user'
         LEFT JOIN relay_shared_agents sender_agent ON sender_agent.workspace=j.workspace AND sender_agent.id=original.author AND original.kind='agent'
         WHERE j.workspace=$1 AND j.reply=$2",
        vec![job.workspace.clone().into(),job.reply.clone().into(),job.requester.clone().into()]
    ).await?.context("Request identity unavailable")?;
    let sender: Option<String> = row.try_get("", "sender")?;
    let sender_kind: Option<String> = row.try_get("", "sender_kind")?;
    let sender_name: Option<String> = row.try_get("", "sender_name")?;
    let class: String = row.try_get("", "work_class")?;
    let origin = match (sender_kind.as_deref(), class.as_str()) {
        (Some("agent"), _) => "agent_handoff",
        (Some("user"), "background") => "background_automation",
        (Some("user"), _) => "human_message",
        _ => "historical_request",
    };
    let initiating_actor = if origin == "background_automation" {
        None
    } else {
        sender.map(|id| json!({"id":id,"kind":sender_kind,"display_name":sender_name}))
    };
    Ok(json!({
        "version":1,"workspace":job.workspace,"agent":job.agent,"task":job.reply,
        "requesting_principal":{
            "id":row.try_get::<String>("","id")?,"kind":"workspace_user",
            "display_name":row.try_get::<Option<String>>("","display_name")?,
            "email":row.try_get::<String>("","email")?
        },
        "conversation":{"id":job.chat,"kind":row.try_get::<String>("","kind")?,
            "thread":row.try_get::<Option<String>>("","thread")?},
        "request_message":row.try_get::<Option<String>>("","request_message")?,
        "work_class":class,"origin":origin,
        "initiating_actor":initiating_actor,
        "communication":super::communication_inbox::context(tx,job).await?
    }))
}

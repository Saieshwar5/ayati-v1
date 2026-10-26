//! Read-only current context projection; never an earlier task's execution receipt.
use super::*;
use crate::identity::agents::config::Settings;
use serde_json::json;

impl PgStore {
    pub(crate) async fn agent_context_preview(
        &self,
        user: &User,
        workspace: &str,
        agent: &str,
        conversation: &str,
    ) -> Result<Value> {
        ensure!(
            uuid::Uuid::parse_str(conversation).is_ok(),
            "Choose a conversation"
        );
        let tx = self.read_tx(workspace).await?;
        execute(&tx, "SET LOCAL statement_timeout = '5s'", vec![]).await?;
        let profile =
            super::agent_profiles::managed_access(&tx, &user.id, workspace, agent, false).await?;
        super::chat::access(&tx, user, workspace, conversation).await?;
        let row = query(
            &tx,
            "SELECT c.kind,c.archived FROM relay_chats c
             JOIN relay_chat_agents a ON a.workspace=c.workspace AND a.chat=c.id AND a.agent=$3
             WHERE c.workspace=$1 AND c.id=$2",
            vec![workspace.into(), conversation.into(), agent.into()],
        )
        .await?
        .context("Agent does not belong to this conversation")?;
        let managed = profile["revision"].as_i64().unwrap_or(0) > 0;
        let kind: String = row.try_get("", "kind")?;
        ensure!(
            managed || kind == "direct",
            "Save this legacy agent profile before previewing channel context"
        );
        let settings: Settings = serde_json::from_value(profile["settings"].clone())?;
        let limit = if managed { 60 } else { 30 };
        let messages = super::conversation_context::recent(
            &tx,
            super::conversation_context::RecentScope {
                workspace,
                chat: conversation,
                agent,
                thread: None,
                since_join: settings.history == "since_join",
                limit,
                before: i64::MAX,
                request: "",
            },
        )
        .await?;
        let notes = if settings.memory {
            super::agent_knowledge::context_notes(&tx, workspace, agent, conversation).await?
        } else {
            vec![]
        };
        let attempts =
            super::context_attempts::inspect(&tx, user, workspace, agent, conversation, None)
                .await?;
        let job = crate::identity::chat::store::Job {
            workspace: workspace.into(),
            agent: agent.into(),
            chat: conversation.into(),
            requester: user.id.clone(),
            reply: String::new(),
            prompt: String::new(),
            claim: None,
        };
        let working = super::working_context::latest(&tx, &job).await?;
        let result = json!({
            "version":1,"mode":"current_preview","workspace":workspace,"agent":agent,
            "conversation":conversation,"kind":kind,"assembled_at":now(),
            "history_policy":settings.history,"message_limit":limit,"history_byte_limit":14000,
            "knowledge_enabled":settings.memory,"paused":settings.paused,
            "archived":row.try_get::<bool>("", "archived")?,"messages":messages,"notes":notes,
            "attempts":attempts,"working":working
        });
        tx.commit().await?;
        Ok(result)
    }
}

impl PgStore {
    pub(crate) async fn agent_context_detail(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        chat: &str,
        task: &str,
        claim: &str,
    ) -> Result<Value> {
        ensure!(
            [chat, task, claim]
                .iter()
                .all(|v| uuid::Uuid::parse_str(v).is_ok()),
            "Invalid context scope"
        );
        let tx = self.read_tx(w).await?;
        super::agent_profiles::managed_access(&tx, &user.id, w, agent, false).await?;
        super::chat::access(&tx, user, w, chat).await?;
        ensure!(
            query(
                &tx,
                "SELECT agent FROM relay_chat_agents WHERE workspace=$1 AND chat=$2 AND agent=$3",
                vec![w.into(), chat.into(), agent.into()]
            )
            .await?
            .is_some(),
            "Agent left this conversation"
        );
        let values =
            super::context_attempts::inspect(&tx, user, w, agent, chat, Some((task, claim)))
                .await?;
        let result = values
            .as_array()
            .and_then(|v| v.first())
            .cloned()
            .context("Saved context unavailable")?;
        tx.commit().await?;
        Ok(result)
    }
}

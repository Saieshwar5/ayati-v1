use super::agent_profiles::managed_access;
use super::*;
use serde::{Deserialize, Serialize};
use serde_json::json;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeInput {
    pub id: String,
    pub chat: Option<String>,
    pub title: String,
    pub content: String,
    pub source_message: Option<String>,
    pub revision: i64,
    pub forgotten: bool,
}
impl PgStore {
    pub async fn agent_knowledge(&self, user: &User, w: &str, id: &str) -> Result<Vec<Value>> {
        let tx = self.read_tx(w).await?;
        managed_access(&tx, &user.id, w, id, false).await?;
        let result=json_rows(&tx,"SELECT to_jsonb(k) AS data FROM relay_agent_knowledge k WHERE k.workspace=$1 AND k.agent=$2 AND NOT k.forgotten AND (k.chat IS NULL OR EXISTS(SELECT 1 FROM relay_chats c WHERE c.workspace=k.workspace AND c.id=k.chat AND (c.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id=$3)))) ORDER BY created_at DESC LIMIT 100",vec![w.into(),id.into(),user.id.clone().into()]).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub async fn save_agent_knowledge(
        &self,
        user: &User,
        w: &str,
        id: &str,
        input: &KnowledgeInput,
    ) -> Result<()> {
        ensure!(
            uuid::Uuid::parse_str(&input.id).is_ok()
                && !input.title.trim().is_empty()
                && input.title.len() <= 160
                && !input.content.trim().is_empty()
                && input.content.len() <= 12000,
            "Use a title and a note up to 12,000 bytes"
        );
        let tx = self.workspace_tx(w).await?;
        managed_access(&tx, &user.id, w, id, true).await?;
        if let Some(chat) = &input.chat {
            super::chat::access(&tx, user, w, chat).await?;
            ensure!(query(&tx,"SELECT agent FROM relay_chat_agents WHERE workspace=$1 AND chat=$2 AND agent=$3",vec![w.into(),chat.clone().into(),id.into()]).await?.is_some(),"Agent does not belong to this conversation");
        }
        if let Some(source) = &input.source_message {
            let chat = input
                .chat
                .as_ref()
                .context("Source messages require a conversation scope")?;
            ensure!(
                query(
                    &tx,
                    "SELECT id FROM relay_chat_messages WHERE workspace=$1 AND chat=$2 AND id=$3",
                    vec![w.into(), chat.clone().into(), source.clone().into()]
                )
                .await?
                .is_some(),
                "Source unavailable"
            );
        }
        let old=query(&tx,"SELECT to_jsonb(k) AS data FROM relay_agent_knowledge k WHERE workspace=$1 AND agent=$2 AND id=$3",vec![w.into(),id.into(),input.id.clone().into()]).await?;
        if let Some(row) = old {
            let saved: Value = row.try_get("", "data")?;
            ensure!(
                saved["chat"] == json!(input.chat)
                    && saved["source_message"] == json!(input.source_message),
                "Knowledge scope and source cannot change; create a separate reviewed note"
            );
            ensure!(
                saved["revision"] == input.revision && saved["forgotten"] == false,
                "Knowledge changed; reload before editing"
            );
            execute(&tx,"UPDATE relay_agent_knowledge SET title=$4,content=$5,forgotten=$6,revision=revision+1 WHERE workspace=$1 AND agent=$2 AND id=$3",vec![w.into(),id.into(),input.id.clone().into(),input.title.clone().into(),input.content.clone().into(),input.forgotten.into()]).await?;
        } else {
            ensure!(input.revision == 0 && !input.forgotten, "Knowledge missing");
            execute(&tx,"INSERT INTO relay_agent_knowledge(workspace,agent,id,chat,title,content,source_message,creator_id,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",vec![w.into(),id.into(),input.id.clone().into(),input.chat.clone().into(),input.title.clone().into(),input.content.clone().into(),input.source_message.clone().into(),user.id.clone().into(),now().into()]).await?;
        }
        if super::context_attempts::exists(&tx).await? {
            super::task_revocation::inaccessible(&tx, w).await?;
        }
        audit(
            &tx,
            &user.id,
            Some(w),
            if input.forgotten {
                "agent_knowledge_forgotten"
            } else {
                "agent_knowledge_saved"
            },
            &input.id,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
    pub async fn agent_context_notes(
        &self,
        user: &User,
        w: &str,
        id: &str,
        chat: &str,
    ) -> Result<String> {
        let tx = self.read_tx(w).await?;
        managed_access(&tx, &user.id, w, id, false).await?;
        super::chat::access(&tx, user, w, chat).await?;
        let notes = context_notes(&tx, w, id, chat).await?;
        tx.commit().await?;
        Ok(notes
            .iter()
            .map(serde_json::to_string)
            .collect::<std::result::Result<Vec<_>, _>>()?
            .join("\n"))
    }
}

// Shared by actual execution and the read-only context preview. Scope first,
// then apply the same deterministic, serialized note budget in both paths.
pub(super) async fn context_notes(
    tx: &DatabaseTransaction,
    w: &str,
    id: &str,
    chat: &str,
) -> Result<Vec<Value>> {
    let notes=json_rows(tx,"SELECT jsonb_build_object('id',id,'revision',revision,'chat',chat,'title',title,'content',content,'source',COALESCE(source_message,'Maintainer note')) AS data FROM relay_agent_knowledge WHERE workspace=$1 AND agent=$2 AND (chat=$3 OR chat IS NULL) AND NOT forgotten ORDER BY created_at DESC,id DESC LIMIT 20",vec![w.into(),id.into(),chat.into()]).await?;
    let mut selected = Vec::new();
    let mut bytes = 0;
    for note in notes {
        let size = serde_json::to_vec(&note)?.len() + usize::from(!selected.is_empty());
        if bytes + size > 16000 {
            break;
        }
        bytes += size;
        selected.push(note);
    }
    Ok(selected)
}

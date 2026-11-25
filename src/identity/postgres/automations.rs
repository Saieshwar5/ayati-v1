//! Where an automation an agent's owner sets up may run (schema 151: in that
//! conversation, as its person).
use super::automation_access::may_ask_in;
use super::*;
use serde_json::json;
impl PgStore {
    pub(crate) fn pooled_execution(&self) -> bool {
        self.pooled
    }
    /// The conversations, at most 100, where this person may ask the agent, so an
    /// automation may run there, and whether this server runs work on its own
    /// (pooled executors).
    pub(crate) async fn post_targets(&self, user: &User, w: &str, agent: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        super::agent_profiles::managed_access(&tx, &user.id, w, agent, true).await?;
        let mut conversations = Vec::new();
        for data in json_rows(&tx,"SELECT jsonb_build_object('id',c.id,'name',c.name,'kind',c.kind) AS data FROM relay_chats c JOIN relay_chat_agents a ON a.workspace=c.workspace AND a.chat=c.id WHERE c.workspace=$1 AND a.agent=$2 AND NOT c.archived AND (c.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id=$3)) ORDER BY c.id LIMIT 101",vec![w.into(),agent.into(),user.id.clone().into()]).await? {
            if may_ask_in(&tx,user,w,agent,data["id"].as_str().unwrap()).await.is_ok() { conversations.push(data); }
        }
        let more = conversations.len() > 100;
        conversations.truncate(100);
        tx.commit().await?;
        Ok(
            json!({"conversations":conversations,"more_conversations":more,"execution_available":self.pooled}),
        )
    }
}

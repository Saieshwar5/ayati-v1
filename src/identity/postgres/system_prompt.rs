//! What decides which Cuckoo system prompt sections an agent gets, read the same
//! way requests read it (`chat/execution.rs`).
use super::*;
use crate::identity::agents::system_prompt::Features;

impl PgStore {
    pub(crate) async fn prompt_features(&self, w: &str, agent: &str) -> Result<Features> {
        Ok(Features {
            business_actions: self.control_enabled(w, agent).await?,
            executor: self.pooled,
            library: self.library_agent_has_items(w, agent).await?,
            web_search: self.web_search_on(w, agent).await?,
            skills: self.agent_skill_listing(w, agent).await?,
        })
    }
}

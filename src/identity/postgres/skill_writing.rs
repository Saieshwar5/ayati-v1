//! What the skill writer may know besides the conversation, read under the
//! person's own access: library skills they can add, every skill name (so new
//! names never collide) and the agent's apps with each action's kind.
use super::*;
use crate::identity::skill_writer::{LibrarySkill, WriterContext};
use serde_json::json;

impl PgStore {
    /// `agent`: a saved agent, whose switched-on apps count; otherwise the
    /// connectors `chosen` for a new agent.
    pub(crate) async fn skill_writer_context(
        &self,
        user: &User,
        w: &str,
        agent: Option<&str>,
        chosen: &[String],
    ) -> Result<WriterContext> {
        let library = self.skills(user, w).await?;
        let tx = self.read_tx(w).await?;
        let names = rows(
            &tx,
            "SELECT name FROM relay_skills WHERE workspace=$1",
            vec![w.into()],
        )
        .await?
        .into_iter()
        .map(|r| r.try_get("", "name"))
        .collect::<Result<Vec<String>, _>>()?;
        let connections = json_rows(
            &tx,
            include_str!("agent_tools.sql"),
            vec![w.into(), agent.unwrap_or("").into()],
        )
        .await?;
        tx.commit().await?;
        let apps = connections
            .into_iter()
            .filter(|c| match agent {
                Some(_) => c["on"] == true,
                None => chosen.iter().any(|id| c["id"] == id.as_str()),
            })
            .map(|c| {
                let actions: Vec<Value> = c["operations"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|op| json!({"title": op["title"], "kind": op["meaning"]}))
                    .collect();
                let name = c["name"]
                    .as_str()
                    .filter(|n| !n.is_empty())
                    .unwrap_or(c["server"].as_str().unwrap_or_default());
                json!({"name": name, "actions": actions})
            })
            .collect();
        let library = ["workspace", "mine"]
            .iter()
            .flat_map(|group| library[*group].as_array().cloned().unwrap_or_default())
            .map(|s| LibrarySkill {
                id: s["id"].as_str().unwrap_or_default().to_owned(),
                name: s["name"].as_str().unwrap_or_default().to_owned(),
                description: s["description"].as_str().unwrap_or_default().to_owned(),
            })
            .collect();
        Ok(WriterContext {
            names,
            library,
            apps,
        })
    }
}

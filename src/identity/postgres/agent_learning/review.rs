//! Agents › Instructions › Knowledge › Lessons from its work: the people who
//! maintain an agent see proposed lessons with their evidence, approve (as
//! proposed or edited), or reject them, and can switch learning off.
use super::super::agent_knowledge::KnowledgeInput;
use super::super::agent_profiles::managed_access;
use super::*;

/// A maintainer's answer to one proposed lesson.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Decision {
    pub approve: bool,
    /// Edited wording; the proposal's own when absent.
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub content: Option<String>,
    /// A new lesson for every conversation instead of only the one it came from.
    #[serde(default)]
    pub everywhere: bool,
}

impl PgStore {
    /// Whether the agent learns, its last run, and its recent proposals.
    pub(crate) async fn agent_learning(&self, user: &User, w: &str, agent: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        managed_access(&tx, &user.id, w, agent, true).await?;
        if !ready(&tx).await? {
            tx.commit().await?;
            return Ok(json!({"available":false,"proposals":[]}));
        }
        let state = query(&tx, "SELECT enabled,last_run_ms,last_result,revision FROM relay_agent_learning WHERE workspace=$1 AND agent=$2",
            vec![w.into(), agent.into()]).await?;
        let mut proposals = json_rows(&tx, "SELECT jsonb_build_object('id',p.id,'title',p.title,'content',p.content,'why',p.why,'kind',p.kind,
                'state',p.state,'created_ms',p.created_ms,'evidence',p.evidence,
                'conversation',CASE WHEN c.id IS NULL THEN NULL ELSE jsonb_build_object('id',c.id,'name',c.name) END,
                'replaces',CASE WHEN k.id IS NULL THEN NULL ELSE jsonb_build_object('id',k.id,'title',k.title,'content',k.content) END) AS data
            FROM relay_knowledge_proposals p
            LEFT JOIN relay_chats c ON c.workspace=p.workspace AND c.id=p.chat
            LEFT JOIN relay_agent_knowledge k ON k.workspace=p.workspace AND k.agent=p.agent AND k.id=p.replaces
            WHERE p.workspace=$1 AND p.agent=$2 ORDER BY p.state='proposed' DESC,p.created_ms DESC LIMIT 50",
            vec![w.into(), agent.into()]).await?;
        // Evidence text only from conversations this person can open.
        let mut open: BTreeMap<String, bool> = BTreeMap::new();
        for proposal in &mut proposals {
            let mut shown = Vec::new();
            for item in proposal["evidence"].as_array().cloned().unwrap_or_default() {
                let chat = item["chat"].as_str().unwrap_or_default().to_owned();
                if !open.contains_key(&chat) {
                    let allowed = super::super::chat::access(&tx, user, w, &chat)
                        .await
                        .is_ok();
                    open.insert(chat.clone(), allowed);
                }
                let mut entry = json!({"kind": item["kind"]});
                if open[&chat]
                    && let Some(row) = query(&tx, "SELECT left(m.content,300) AS excerpt,c.name FROM relay_chat_messages m JOIN relay_chats c ON c.workspace=m.workspace AND c.id=m.chat WHERE m.workspace=$1 AND m.id=$2 AND m.chat=$3",
                        vec![w.into(), item["message"].as_str().unwrap_or_default().into(), chat.clone().into()]).await?
                {
                    entry["excerpt"] = json!(row.try_get::<String>("", "excerpt")?);
                    entry["conversation"] = json!(row.try_get::<String>("", "name")?);
                }
                shown.push(entry);
            }
            proposal["evidence"] = json!(shown);
        }
        tx.commit().await?;
        Ok(json!({
            "available": true,
            "enabled": state.as_ref().map(|r| r.try_get::<bool>("", "enabled")).transpose()?.unwrap_or(true),
            "last_run_ms": state.as_ref().map(|r| r.try_get::<Option<i64>>("", "last_run_ms")).transpose()?.flatten(),
            "last_result": state.as_ref().map(|r| r.try_get::<Option<String>>("", "last_result")).transpose()?.flatten(),
            "revision": state.as_ref().map(|r| r.try_get::<i64>("", "revision")).transpose()?.unwrap_or(0),
            "proposals": proposals,
        }))
    }

    /// Switches learning on or off for this agent.
    pub(crate) async fn set_agent_learning(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        enabled: bool,
        revision: i64,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        managed_access(&tx, &user.id, w, agent, true).await?;
        ensure!(
            ready(&tx).await?,
            "Run workspace-db migrate to use learning"
        );
        // The first change creates the row; later ones must name its revision.
        let created = revision == 0
            && execute(&tx, "INSERT INTO relay_agent_learning(workspace,agent,enabled,revision) VALUES($1,$2,$3,1) ON CONFLICT DO NOTHING",
                vec![w.into(), agent.into(), enabled.into()]).await? == 1;
        let changed = created
            || execute(&tx, "UPDATE relay_agent_learning SET enabled=$3,revision=revision+1 WHERE workspace=$1 AND agent=$2 AND revision=$4",
                vec![w.into(), agent.into(), enabled.into(), revision.into()]).await? == 1;
        ensure!(changed, "Learning changed; reload before trying again");
        audit(
            &tx,
            &user.id,
            Some(w),
            if enabled {
                "agent_learning_on"
            } else {
                "agent_learning_off"
            },
            agent,
        )
        .await?;
        tx.commit().await?;
        self.agent_learning(user, w, agent).await
    }

    /// Approves (as proposed or edited) or rejects one lesson. An approved new
    /// lesson becomes a note for its conversation, or every conversation; an
    /// update replaces the note it is about. A decided lesson stays decided.
    pub(crate) async fn decide_lesson(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        id: &str,
        decision: &Decision,
    ) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        managed_access(&tx, &user.id, w, agent, true).await?;
        let proposal = query(&tx, "SELECT p.state,p.chat,p.title,p.content,p.replaces,k.revision AS note_revision,k.chat AS note_chat
            FROM relay_knowledge_proposals p LEFT JOIN relay_agent_knowledge k ON k.workspace=p.workspace AND k.agent=p.agent AND k.id=p.replaces AND NOT k.forgotten
            WHERE p.workspace=$1 AND p.agent=$2 AND p.id=$3", vec![w.into(), agent.into(), id.into()]).await?
            .context("Lesson not found")?;
        tx.commit().await?;
        let state: String = proposal.try_get("", "state")?;
        if state != "proposed" {
            return Ok(json!({"id":id,"state":state}));
        }
        let note = if decision.approve {
            let replaces: Option<String> = proposal.try_get("", "replaces")?;
            let revision: Option<i64> = proposal.try_get("", "note_revision")?;
            ensure!(
                replaces.is_none() || revision.is_some(),
                "The note this lesson updates was removed; reject it instead"
            );
            let note = replaces.clone().unwrap_or_else(|| id.to_owned());
            let chat = match replaces {
                Some(_) => proposal.try_get("", "note_chat")?,
                None if decision.everywhere => None,
                None => proposal.try_get("", "chat")?,
            };
            let input = KnowledgeInput {
                id: note.clone(),
                chat,
                title: decision
                    .title
                    .clone()
                    .unwrap_or(proposal.try_get("", "title")?),
                content: decision
                    .content
                    .clone()
                    .unwrap_or(proposal.try_get("", "content")?),
                source_message: None,
                revision: revision.unwrap_or(0),
                forgotten: false,
            };
            self.save_agent_knowledge(user, w, agent, &input).await?;
            Some(note)
        } else {
            None
        };
        let tx = self.workspace_tx(w).await?;
        let state = if note.is_some() {
            "approved"
        } else {
            "rejected"
        };
        execute(&tx, "UPDATE relay_knowledge_proposals SET state=$4,note=$5,decided_by=$6,decided_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint
            WHERE workspace=$1 AND agent=$2 AND id=$3 AND state='proposed'",
            vec![w.into(), agent.into(), id.into(), state.into(), note.clone().into(), user.id.clone().into()]).await?;
        if let Some(note) = &note {
            execute(&tx, "UPDATE relay_agent_knowledge SET learned_from=$4 WHERE workspace=$1 AND agent=$2 AND id=$3",
                vec![w.into(), agent.into(), note.clone().into(), id.into()]).await?;
        }
        audit(
            &tx,
            &user.id,
            Some(w),
            if note.is_some() {
                "agent_lesson_approved"
            } else {
                "agent_lesson_rejected"
            },
            agent,
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"id":id,"state":state,"note":note}))
    }
}

use super::*;
use crate::{identity::chat::store::Job, panels::PanelDraft};
use serde_json::json;
impl PgStore {
    pub(crate) async fn save_personal_panel(&self, job: &Job, input: &PanelDraft) -> Result<Value> {
        input.validate()?;
        ensure!(
            !input.publish && !input.updates_enabled,
            "Personal Home drafts require exact human review; update policies are not supported yet"
        );
        let tx = self.workspace_tx(&job.workspace).await?;
        super::control_actions::authorize(&tx, job, true).await?;
        let hash = crate::identity::events::model::fingerprint(
            &json!({"definition":input.definition(),"expected_revision":input.expected_revision,"sources":input.sources}),
        )?;
        if let Some(r)=query(&tx,"SELECT panel,revision FROM relay_home_panel_versions WHERE workspace=$1 AND task=$2 AND request_hash=$3",vec![job.workspace.clone().into(),job.reply.clone().into(),hash.clone().into()]).await?{
   let result=json!({"id":r.try_get::<String>("","panel")?,"revision":r.try_get::<i64>("","revision")?,"state":"saved_draft","message":"Exact saved draft receipt. Human review is still required; do not create the same view again."});tx.commit().await?;return Ok(result)
  }
        super::handoff_controls::ready_for_new_work(&tx, job).await?;
        ensure!(query(&tx,"SELECT j.reply FROM relay_chat_jobs j WHERE j.workspace=$1 AND j.reply=$2 AND (EXISTS(SELECT 1 FROM relay_task_questions q WHERE q.workspace=j.workspace AND q.reply=j.reply AND q.state='pending') OR EXISTS(SELECT 1 FROM relay_control_actions a WHERE a.workspace=j.workspace AND a.job=j.reply AND a.state IN ('pending','executing','uncertain')))",vec![job.workspace.clone().into(),job.reply.clone().into()]).await?.is_none(),"Resolve saved input, approval and uncertain outcomes before preparing another view");
        let person = User {
            id: job.requester.clone(),
            email: String::new(),
            display_name: None,
        };
        let profile = super::agent_profiles::managed_access(
            &tx,
            &person.id,
            &job.workspace,
            &job.agent,
            false,
        )
        .await?;
        ensure!(
            profile["scope"] == "workspace" && profile["revision"].as_i64().is_some_and(|n| n > 0),
            "Personal panels require a managed workspace agent"
        );
        let old = query(
            &tx,
            "SELECT owner_id,agent,chat,revision FROM relay_home_panels WHERE workspace=$1 AND id=$2",
            vec![job.workspace.clone().into(), input.id.clone().into()],
        )
        .await?;
        let previous = if let Some(row) = old {
            ensure!(
                row.try_get::<String>("", "owner_id")? == job.requester
                    && row.try_get::<String>("", "agent")? == job.agent
                    && row.try_get::<String>("", "chat")? == job.chat,
                "Panel unavailable for this person and agent"
            );
            ensure!(
                row.try_get::<i64>("", "revision")? == input.expected_revision,
                "Panel revision changed; inspect current panel identities before revising"
            );
            ensure!(
                input.expected_revision < 100,
                "Panel revision history reached its supported limit"
            );
            Some(query(&tx,"SELECT provenance FROM relay_home_panel_versions WHERE workspace=$1 AND panel=$2 AND revision=$3",vec![job.workspace.clone().into(),input.id.clone().into(),input.expected_revision.into()]).await?.context("Panel history unavailable")?.try_get::<Value>("","provenance")?)
        } else {
            ensure!(
                input.expected_revision == 0,
                "New panels start with expected_revision=0"
            );
            ensure!(query(&tx,"SELECT count(*) AS n FROM relay_home_panels WHERE workspace=$1 AND owner_id=$2",vec![job.workspace.clone().into(),job.requester.clone().into()]).await?.unwrap().try_get::<i64>("","n")?<32,"Personal Home reached its supported 32-panel collection bound");
            None
        };
        let provenance =
            super::home_sources::capture(&tx, job, previous.as_ref(), &input.sources).await?;
        let revision = input.expected_revision + 1;
        execute(&tx,"INSERT INTO relay_home_panels(workspace,id,owner_id,agent,revision,chat) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT(workspace,id) DO UPDATE SET revision=excluded.revision",vec![job.workspace.clone().into(),input.id.clone().into(),job.requester.clone().into(),job.agent.clone().into(),revision.into(),job.chat.clone().into()]).await?;
        execute(&tx,"INSERT INTO relay_home_panel_versions(workspace,panel,revision,task,request_hash,definition,provenance) VALUES($1,$2,$3,$4,$5,$6,$7)",vec![job.workspace.clone().into(),input.id.clone().into(),revision.into(),job.reply.clone().into(),hash.into(),input.definition().into(),provenance.into()]).await?;
        audit(
            &tx,
            &job.requester,
            Some(&job.workspace),
            "home_panel_draft_saved",
            &input.id,
        )
        .await?;
        tx.commit().await?;
        Ok(
            json!({"id":input.id,"revision":revision,"state":"saved_draft","message":"Personal Home draft saved. Ask the original person to review it on Home and choose Keep on Home. It is not published and no schedule or source authority was granted."}),
        )
    }
    pub(crate) async fn personal_panels(&self, job: &Job) -> Result<Value> {
        let tx = self.read_tx(&job.workspace).await?;
        super::control_actions::authorize(&tx, job, true).await?;
        let mut result = vec![];
        for row in rows(&tx,"SELECT id,revision,active_revision FROM relay_home_panels WHERE workspace=$1 AND owner_id=$2 AND agent=$3 AND chat=$4 ORDER BY created_ms,id LIMIT 32",vec![job.workspace.clone().into(),job.requester.clone().into(),job.agent.clone().into(),job.chat.clone().into()]).await?{
            result.push(json!({"id":row.try_get::<String>("","id")?,"revision":row.try_get::<i64>("","revision")?,"active_revision":row.try_get::<Option<i64>>("","active_revision")?}));
        }
        tx.commit().await?;
        Ok(
            json!({"panels":result,"message":"Personal panel identities in this source conversation only. No saved titles or facts are disclosed. Previous provenance is retained on revisions. Inspect current facts independently; saved views do not grant source access or prove freshness."}),
        )
    }
}

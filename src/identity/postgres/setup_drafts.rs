//! Draft persistence, side-effect-free previews and atomic activation.
use super::*;
use crate::identity::control::setup_model::{Activate, Draft, Edit, Preview, Start};
use serde_json::json;
pub(super) const VIEW: &str = "jsonb_build_object('id',id,'revision',revision,'data',data,'state',state,'receipt',receipt,'previewed',COALESCE(preview_revision=revision,false))";

pub(super) async fn read(
    db: &impl ConnectionTrait,
    user: &User,
    w: &str,
    agent: &str,
    id: &str,
) -> Result<Value> {
    super::agent_profiles::managed_access(db, &user.id, w, agent, true).await?;
    query(db,&format!("SELECT {VIEW} AS data FROM relay_control_drafts WHERE workspace=$1 AND agent=$2 AND user_id=$3 AND id=$4"),vec![w.into(),agent.into(),user.id.clone().into(),id.into()]).await?.context("Draft unavailable")?.try_get("","data").map_err(Into::into)
}
async fn validate(
    db: &impl ConnectionTrait,
    user: &User,
    w: &str,
    agent: &str,
    draft: &Draft,
) -> Result<Vec<(crate::identity::control::model::Rule, String)>> {
    let current = super::setup_context::select(
        super::setup_context::context(db, user, w, agent).await?,
        &draft.request.connections,
        &draft.request.chat,
    )?;
    ensure!(
        serde_json::to_value(current)? == serde_json::to_value(&draft.context)?,
        "Agent, account or permissions changed. Start a new review"
    );
    let mut result = Vec::new();
    for rule in draft.rules()? {
        let approver = super::control_rules::validate_rule(db, user, w, agent, &rule).await?;
        result.push((rule, approver));
    }
    Ok(result)
}
impl PgStore {
    pub(crate) async fn setup_last_draft(
        &self,
        user: &User,
        w: &str,
        agent: &str,
    ) -> Result<Option<Value>> {
        let tx = self.read_tx(w).await?;
        super::agent_profiles::managed_access(&tx, &user.id, w, agent, true).await?;
        let row = query(&tx,&format!("SELECT {VIEW} AS data FROM relay_control_drafts WHERE workspace=$1 AND agent=$2 AND user_id=$3 ORDER BY created DESC LIMIT 1"),vec![w.into(),agent.into(),user.id.clone().into()]).await?;
        let result = row.map(|r| r.try_get("", "data")).transpose()?;
        tx.commit().await?;
        Ok(result)
    }
    pub(crate) async fn setup_existing(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        input: &Start,
    ) -> Result<Option<Value>> {
        let tx = self.read_tx(w).await?;
        super::agent_profiles::managed_access(&tx, &user.id, w, agent, true).await?;
        let row = query(&tx,&format!("SELECT {VIEW} AS data FROM relay_control_drafts WHERE workspace=$1 AND agent=$2 AND user_id=$3 AND id=$4"),vec![w.into(),agent.into(),user.id.clone().into(),input.id.clone().into()]).await?;
        let result = row.map(|r| r.try_get::<Value>("", "data")).transpose()?;
        if let Some(value) = &result {
            ensure!(
                value["data"]["request"] == serde_json::to_value(input)?,
                "This draft identity belongs to a different request"
            );
        }
        tx.commit().await?;
        Ok(result)
    }
    pub(crate) async fn setup_create(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        draft: &Draft,
    ) -> Result<Value> {
        draft.request.validate()?;
        draft.rules()?;
        let tx = self.workspace_tx(w).await?;
        validate(&tx, user, w, agent, draft).await?;
        let data = serde_json::to_value(draft)?;
        let old = query(
            &tx,
            "SELECT data FROM relay_control_drafts WHERE workspace=$1 AND id=$2",
            vec![w.into(), draft.request.id.clone().into()],
        )
        .await?;
        if let Some(old) = old {
            ensure!(
                old.try_get::<Value>("", "data")? == data,
                "Draft identity already used"
            );
        } else {
            execute(&tx,"INSERT INTO relay_control_drafts(workspace,agent,user_id,id,data) VALUES($1,$2,$3,$4,$5)",vec![w.into(),agent.into(),user.id.clone().into(),draft.request.id.clone().into(),data.into()]).await?;
            audit(
                &tx,
                &user.id,
                Some(w),
                "control_draft_created",
                &draft.request.id,
            )
            .await?;
        }
        let result = read(&tx, user, w, agent, &draft.request.id).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub(crate) async fn setup_draft(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        id: &str,
    ) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let result = read(&tx, user, w, agent, id).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub(crate) async fn setup_edit(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        id: &str,
        input: &Edit,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        let old = read(&tx, user, w, agent, id).await?;
        ensure!(
            old["state"] == "draft",
            "Activated reviews cannot be edited"
        );
        let mut draft: Draft = serde_json::from_value(old["data"].clone())?;
        let same = serde_json::to_value(&draft.choices)? == serde_json::to_value(&input.choices)?;
        let retry = super::workspace_connections::stale_revision(
            old["revision"].as_i64().unwrap(),
            input.revision,
            same,
        )?;
        if !retry {
            draft.choices = input.choices.clone();
            draft.rules()?;
            execute(&tx,"UPDATE relay_control_drafts SET data=$3,revision=revision+1,preview_revision=NULL WHERE workspace=$1 AND id=$2",vec![w.into(),id.into(),serde_json::to_value(draft)?.into()]).await?;
            audit(&tx, &user.id, Some(w), "control_draft_saved", id).await?;
        }
        let result = read(&tx, user, w, agent, id).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub(crate) async fn setup_preview(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        id: &str,
        input: &Preview,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        let old = read(&tx, user, w, agent, id).await?;
        ensure!(
            old["state"] == "draft" && old["revision"] == input.revision,
            "Draft changed; save and preview it again"
        );
        let draft: Draft = serde_json::from_value(old["data"].clone())?;
        let rules = validate(&tx, user, w, agent, &draft).await?;
        let results: Vec<_> = rules.iter().map(|(r,approver)| json!({"connection":r.connection,"operation":r.operation,"decision":r.mode,"approver":if r.mode=="ask" {Some(approver)} else {None}})).collect();
        let example = input.example.as_ref().map(|example| {
            let permission = draft.context.accounts.iter().find(|a| a.id==example.connection).and_then(|a| a.permissions.iter().find(|p| p.operation.id==example.operation));
            let result = results.iter().find(|r| r["connection"]==example.connection && r["operation"]==example.operation);
            match (permission,result) {
                (Some(p),Some(result)) => match p.operation.arguments(&example.arguments) { Ok(()) => result.clone(), Err(_) => json!({"decision":"block","reason":"Example fields do not match the supported operation"}) },
                _ => json!({"decision":"block","reason":"Unreviewed or unselected operation is unavailable"}),
            }
        });
        execute(&tx,"UPDATE relay_control_drafts SET preview_revision=revision WHERE workspace=$1 AND id=$2",vec![w.into(),id.into()]).await?;
        tx.commit().await?;
        Ok(
            json!({"revision":input.revision,"results":results,"example":example,"business_operations_executed":0}),
        )
    }
    pub(crate) async fn setup_activate(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        id: &str,
        input: &Activate,
    ) -> Result<Value> {
        ensure!(input.reviewed, "Review the permissions before activating");
        let tx = self.workspace_tx(w).await?;
        let old = read(&tx, user, w, agent, id).await?;
        ensure!(
            old["revision"] == input.revision,
            "Draft changed; preview it again"
        );
        if old["state"] == "activated" {
            tx.commit().await?;
            return Ok(old["receipt"].clone());
        }
        ensure!(
            old["previewed"] == true,
            "Preview this saved draft before activating"
        );
        let draft: Draft = serde_json::from_value(old["data"].clone())?;
        let rules = validate(&tx, user, w, agent, &draft).await?;
        // All authorization checks finish before any standing permission changes.
        for (rule, approver) in &rules {
            super::control_rules::write_rule(&tx, user, w, agent, rule, approver).await?;
        }
        let receipt = json!({"draft":id,"revision":input.revision,"activated_by":user.id,"agent":agent,"chat":draft.request.chat,"operations":rules.len(),"at":now()});
        execute(&tx,"UPDATE relay_control_drafts SET state='activated',receipt=$3 WHERE workspace=$1 AND id=$2",vec![w.into(),id.into(),receipt.clone().into()]).await?;
        audit(&tx, &user.id, Some(w), "control_draft_activated", id).await?;
        tx.commit().await?;
        Ok(receipt)
    }
}

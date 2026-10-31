//! Panel maintenance is an additional ceiling, never a source of business grants.
use super::*;
use crate::identity::{chat::model::Job, control::model::ActionRequest};
use serde_json::json;

pub(super) async fn binding(db: &impl ConnectionTrait, job: &Job) -> Result<Option<Value>> {
    if !query(
        db,
        "SELECT to_regclass('relay_home_update_runs') IS NOT NULL AS ready",
        vec![],
    )
    .await?
    .unwrap()
    .try_get::<bool>("", "ready")?
    {
        return Ok(None);
    }
    query(db,"SELECT to_jsonb(r)||jsonb_build_object('policy',to_jsonb(u),'owner',p.owner_id,'agent',p.agent,'chat',p.chat,'active_revision',p.active_revision,'definition_revision',p.revision,'basis',v.provenance,'definition',v.definition) AS data FROM relay_home_update_runs r JOIN relay_home_panels p ON p.workspace=r.workspace AND p.id=r.panel JOIN relay_home_maintenance u ON u.workspace=p.workspace AND u.panel=p.id JOIN relay_home_panel_versions v ON v.workspace=r.workspace AND v.panel=r.panel AND v.revision=r.panel_revision WHERE r.workspace=$1 AND r.reply=$2",vec![job.workspace.clone().into(),job.reply.clone().into()]).await?.map(|r|r.try_get("","data").map_err(Into::into)).transpose()
}
pub(super) async fn source_allowed(db: &impl ConnectionTrait, job: &Job) -> Result<bool> {
    let Some(data) = binding(db, job).await? else {
        return Ok(true);
    };
    Ok(data["owner"] == job.requester
        && Box::pin(super::home_sources::allowed(
            db,
            &job.workspace,
            &job.requester,
            &data["basis"],
        ))
        .await?)
}
pub(super) async fn active(db: &impl ConnectionTrait, job: &Job) -> Result<Option<Value>> {
    let Some(data) = binding(db, job).await? else {
        return Ok(None);
    };
    ensure!(
        data["owner"] == job.requester && data["agent"] == job.agent && data["chat"] == job.chat,
        "Panel update principal changed"
    );
    ensure!(
        data["policy"]["enabled"] == true
            && data["policy"]["revision"] == data["policy_revision"]
            && data["policy"]["panel_revision"] == data["panel_revision"]
            && data["active_revision"] == data["panel_revision"]
            && data["definition_revision"] == data["panel_revision"],
        "Panel update agreement changed or was paused"
    );
    ensure!(
        source_allowed(db, job).await?,
        "Panel source access changed"
    );
    Ok(Some(data))
}
pub(super) async fn action(
    db: &impl ConnectionTrait,
    job: &Job,
    input: &ActionRequest,
) -> Result<()> {
    if let Some(data) = active(db, job).await? {
        ensure!(
            data["policy"]["scopes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| s["connection"] == input.connection
                    && s["operation"] == input.operation
                    && s["arguments"] == input.arguments),
            "Panel updates can only repeat their reviewed exact source reads"
        );
    }
    Ok(())
}
pub(super) async fn scopes(
    db: &impl ConnectionTrait,
    w: &str,
    provenance: &Value,
) -> Result<Value> {
    let mut scopes = vec![];
    for source in provenance["operations"]
        .as_array()
        .context("Panel sources unavailable")?
    {
        let row=query(db,"SELECT jsonb_build_object('connection',a.connection,'operation',a.operation,'arguments',a.arguments,'rule_revision',a.rule_revision,'connection_revision',a.connection_revision) AS data FROM relay_control_actions a JOIN relay_control_effective_rules r ON r.workspace=a.workspace AND r.agent=a.agent AND r.connection=a.connection AND r.operation=a.operation WHERE a.workspace=$1 AND a.id=$2 AND a.effect='read' AND a.state='completed' AND r.mode='allow' AND r.revision=a.rule_revision",vec![w.into(),source["id"].as_str().context("Invalid panel source")?.into()]).await?.context("Automatic updates require currently allowed read receipts only; review a new read-based panel first")?;
        let scope: Value = row.try_get("", "data")?;
        if !scopes.contains(&scope) {
            scopes.push(scope);
        }
    }
    ensure!(
        !scopes.is_empty() && scopes.len() <= 16,
        "Review a panel with 1–16 allowed source reads before enabling updates"
    );
    Ok(json!(scopes))
}
pub(super) async fn panel_schedule(db: &impl ConnectionTrait, w: &str, id: &str) -> Result<bool> {
    if !query(
        db,
        "SELECT to_regclass('relay_home_schedule_bindings') IS NOT NULL AS ready",
        vec![],
    )
    .await?
    .unwrap()
    .try_get::<bool>("", "ready")?
    {
        return Ok(false);
    }
    Ok(query(
        db,
        "SELECT schedule FROM relay_home_schedule_bindings WHERE workspace=$1 AND schedule=$2",
        vec![w.into(), id.into()],
    )
    .await?
    .is_some())
}
impl PgStore {
    pub(crate) async fn home_maintenance_tool(&self, job: &Job, tool: &str) -> Result<()> {
        if active(&self.db, job).await?.is_some() {
            ensure!(
                [
                    "operations",
                    "request_action",
                    "home_panel_update_context",
                    "refresh_home_panel"
                ]
                .contains(&tool),
                "This panel update permits reviewed reads and its saved snapshot only"
            );
        }
        Ok(())
    }
    pub(crate) async fn home_update_context(&self, job: &Job) -> Result<Value> {
        let tx = self.read_tx(&job.workspace).await?;
        super::control_actions::authorize(&tx, job, true).await?;
        let data = active(&tx, job)
            .await?
            .context("This task is not a Home panel update")?;
        let result = json!({"definition":data["definition"],"instructions":data["policy"]["instructions"],"reads":data["policy"]["scopes"],"message":"Perform every reviewed read, then call refresh_home_panel with one section matching this definition. Never infer that old facts are current. This task cannot create panels, hand off work or perform writes."});
        tx.commit().await?;
        Ok(result)
    }
}

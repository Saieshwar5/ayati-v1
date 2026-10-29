use super::*;
use crate::identity::{
    chat::model::{Job, NewMessage},
    home_panels::maintenance::Update,
};
use serde_json::json;

async fn unfinished(db: &impl ConnectionTrait, w: &str, panel: &str) -> Result<Option<String>> {
    query(db,"SELECT r.reply FROM relay_home_update_runs r JOIN relay_chat_jobs j ON j.workspace=r.workspace AND j.reply=r.reply JOIN relay_home_maintenance u ON u.workspace=r.workspace AND u.panel=r.panel AND u.revision=r.policy_revision WHERE r.workspace=$1 AND r.panel=$2 AND j.state<>'finished' ORDER BY r.created_ms DESC LIMIT 1",vec![w.into(),panel.into()]).await?.map(|r|r.try_get("","reply").map_err(Into::into)).transpose()
}
pub(super) async fn cancel_unstarted(tx: &DatabaseTransaction, w: &str, panel: &str) -> Result<()> {
    for row in rows(tx,"UPDATE relay_chat_jobs j SET state='finished',lease_until=0,finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE j.workspace=$1 AND j.state='queued' AND j.run_cycle=0 AND j.attempts=0 AND EXISTS(SELECT 1 FROM relay_home_update_runs r WHERE r.workspace=j.workspace AND r.reply=j.reply AND r.panel=$2) RETURNING j.reply",vec![w.into(),panel.into()]).await? {
        execute(tx,"UPDATE relay_chat_messages SET state='cancelled',content='Personal panel update cancelled because its agreement changed.' WHERE workspace=$1 AND id=$2 AND state='queued'",vec![w.into(),row.try_get::<String>("","reply")?.into()]).await?;
    }
    Ok(())
}
pub(super) async fn bind(db: &impl ConnectionTrait, w: &str, panel: &str, job: &Job) -> Result<()> {
    ensure!(execute(db,"INSERT INTO relay_home_update_runs(workspace,panel,reply,policy_revision,panel_revision,expected_generation) SELECT workspace,panel,$3,revision,panel_revision,generation FROM relay_home_maintenance WHERE workspace=$1 AND panel=$2 AND enabled",vec![w.into(),panel.into(),job.reply.clone().into()]).await?==1,"Panel update agreement unavailable");
    Ok(())
}
pub(super) async fn schedule_authority(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    sid: &str,
) -> Result<()> {
    let row = query(
        tx,
        "SELECT panel FROM relay_home_schedule_bindings WHERE workspace=$1 AND schedule=$2",
        vec![w.into(), sid.into()],
    )
    .await?
    .context("Panel routine unavailable")?;
    let p =
        super::home_maintenance::owned(tx, user, w, &row.try_get::<String>("", "panel")?).await?;
    ensure!(
        p["maintenance"]["enabled"] == true && p["maintenance"]["schedule"] == sid,
        "Panel routine was replaced or paused"
    );
    super::home_maintenance::authorized(tx, user, w, &p).await
}
pub(super) async fn schedule_ready(tx: &DatabaseTransaction, w: &str, sid: &str) -> Result<bool> {
    if !super::home_update_access::panel_schedule(tx, w, sid).await? {
        return Ok(true);
    }
    let Some(row)=query(tx,"SELECT p.id,p.owner_id FROM relay_home_panels p JOIN relay_home_maintenance u ON u.workspace=p.workspace AND u.panel=p.id WHERE u.workspace=$1 AND u.schedule=$2 AND u.enabled",vec![w.into(),sid.into()]).await? else {return Ok(false)};
    let person = User {
        id: row.try_get("", "owner_id")?,
        email: String::new(),
        display_name: None,
    };
    let panel: String = row.try_get("", "id")?;
    let p = super::home_maintenance::owned(tx, &person, w, &panel).await?;
    if let Err(error) = super::home_maintenance::authorized(tx, &person, w, &p).await {
        if error.is::<sea_orm::DbErr>() {
            return Err(error);
        }
        execute(tx,"UPDATE relay_agent_schedules SET state='blocked',revision=revision+1,detail='Panel source or agreement changed. Review its updates on Home.' WHERE workspace=$1 AND id=$2",vec![w.into(),sid.into()]).await?;
        return Ok(false);
    }
    Ok(unfinished(tx, w, &panel).await?.is_none())
}
pub(super) async fn bind_schedule(
    db: &impl ConnectionTrait,
    w: &str,
    sid: &str,
    job: &Job,
) -> Result<()> {
    if !super::home_update_access::panel_schedule(db, w, sid).await? {
        return Ok(());
    }
    if let Some(r) = query(
        db,
        "SELECT panel FROM relay_home_schedule_bindings WHERE workspace=$1 AND schedule=$2",
        vec![w.into(), sid.into()],
    )
    .await?
    {
        bind(db, w, &r.try_get::<String>("", "panel")?, job).await?;
    }
    Ok(())
}
impl PgStore {
    pub(crate) async fn update_home_panel(
        &self,
        user: &User,
        w: &str,
        id: &str,
        input: &Update,
    ) -> Result<Value> {
        crate::identity::automations::model::uuid(&input.request)?;
        let tx = self.workspace_tx(w).await?;
        let p = super::home_maintenance::owned(&tx, user, w, id).await?;
        let payload = json!({"update":id,"input":input});
        if let Some(saved) =
            super::home_receipts::retry(&tx, user, w, &input.request, &payload).await?
        {
            tx.commit().await?;
            return Ok(saved);
        }
        ensure!(
            self.pooled
                && p["maintenance"]["enabled"] == true
                && p["maintenance"]["revision"] == input.revision,
            "Panel update agreement changed or is paused"
        );
        super::home_maintenance::authorized(&tx, user, w, &p).await?;
        let reply = if let Some(reply) = unfinished(&tx, w, id).await? {
            reply
        } else {
            let input=NewMessage{id:uuid::Uuid::new_v4().to_string(),content:"Refresh my personal Home panel using home_panel_update_context, its reviewed reads and refresh_home_panel.".into(),agent:Some(p["agent"].as_str().unwrap().into()),thread:None,attachments:vec![]};
            let job = self
                .background_in(&tx, user, w, p["chat"].as_str().unwrap(), &input)
                .await?
                .context("Panel update was not accepted")?;
            bind(&tx, w, id, &job).await?;
            job.reply
        };
        let result = json!({"reply":reply,"state":"queued"});
        super::home_receipts::remember(&tx, user, w, &input.request, &payload, &result).await?;
        tx.commit().await?;
        Ok(result)
    }
}

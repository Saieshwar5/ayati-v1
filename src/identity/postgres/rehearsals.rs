//! Exact personal sample receipts; no conversation or business job is created.
use super::*;
use crate::identity::rehearsals::model::{Start, fingerprint, uuid};
use serde_json::json;

pub(super) async fn record(tx: &DatabaseTransaction, w: &str, id: &str) -> Result<Value> {
    query(tx,"SELECT to_jsonb(r)-'prepared'-'request_hash'-'claim' AS data FROM relay_agent_rehearsals r WHERE workspace=$1 AND id=$2",vec![w.into(),id.into()]).await?.context("Sample unavailable")?.try_get("","data").map_err(Into::into)
}
pub(super) async fn read_in(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    agent: &str,
    id: &str,
) -> Result<Value> {
    let mut data = record(tx, w, id).await?;
    let sources = super::rehearsal_context::readable(tx, user, w, agent, &data).await?;
    data.as_object_mut().unwrap().remove("lease_until");
    data["sources"] = json!(sources);
    Ok(data)
}
async fn existing(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    agent: &str,
    input: &Start,
) -> Result<Option<Value>> {
    let row = query(
        tx,
        "SELECT actor,agent,request_hash FROM relay_agent_rehearsals WHERE workspace=$1 AND id=$2",
        vec![w.into(), input.id.clone().into()],
    )
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    ensure!(
        row.try_get::<String>("", "actor")? == user.id
            && row.try_get::<String>("", "agent")? == agent,
        "Sample unavailable"
    );
    ensure!(
        row.try_get::<String>("", "request_hash")? == fingerprint(input)?,
        "This sample identity belongs to a different request"
    );
    Ok(Some(read_in(tx, user, w, agent, &input.id).await?))
}
impl PgStore {
    pub(crate) async fn rehearsal_existing(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        input: &Start,
    ) -> Result<Option<Value>> {
        input.validate()?;
        let tx = self.read_tx(w).await?;
        super::agent_profiles::managed_access(&tx, &user.id, w, agent, true).await?;
        let result = existing(&tx, user, w, agent, input).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub(crate) async fn start_rehearsal(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        input: &Start,
    ) -> Result<Value> {
        input.validate()?;
        let tx = self.workspace_tx(w).await?;
        let profile = super::agent_profiles::managed_access(&tx, &user.id, w, agent, true).await?;
        if let Some(result) = existing(&tx, user, w, agent, input).await? {
            tx.commit().await?;
            return Ok(result);
        }
        ensure!(
            profile["revision"] == input.agent_revision,
            "Agent changed; refresh its job before starting a sample"
        );
        let counts=query(&tx,"SELECT count(*) AS queued,count(*) FILTER(WHERE actor=$2 AND agent=$3) AS own FROM relay_agent_rehearsals WHERE workspace=$1 AND state IN ('queued','running')",vec![w.into(),user.id.clone().into(),agent.into()]).await?.unwrap();
        ensure!(
            counts.try_get::<i64>("", "own")? == 0,
            "A sample for this agent is already active; open or stop it first"
        );
        ensure!(
            counts.try_get::<i64>("", "queued")? < 10,
            "This workspace's sample queue is full; try again later"
        );
        let (prepared, notes) =
            super::rehearsal_context::prepare(&tx, user, w, agent, input, &profile).await?;
        execute(&tx,"INSERT INTO relay_agent_rehearsals(workspace,id,agent,actor,request_hash,input,profile,note_refs,prepared) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",vec![w.into(),input.id.clone().into(),agent.into(),user.id.clone().into(),fingerprint(input)?.into(),serde_json::to_value(input)?.into(),super::rehearsal_context::profile_snapshot(&profile).into(),notes.into(),serde_json::to_value(&prepared)?.into()]).await?;
        super::executor_tasks::enqueue(
            &tx,
            super::executor_tasks::Purpose::Sample,
            &user.id,
            &prepared,
        )
        .await?;
        audit(&tx, &user.id, Some(w), "agent_sample_requested", &input.id).await?;
        let result = read_in(&tx, user, w, agent, &input.id).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub(crate) async fn rehearsal(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        id: &str,
    ) -> Result<Value> {
        uuid(id)?;
        let tx = self.read_tx(w).await?;
        let result = read_in(&tx, user, w, agent, id).await?;
        tx.commit().await?;
        Ok(result)
    }
    pub(crate) async fn rehearsal_options(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        search: &str,
    ) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let profile = super::agent_profiles::managed_access(&tx, &user.id, w, agent, true).await?;
        let mut sources = super::library::rehearsals::options(&tx, user, w, agent, search).await?;
        let more = sources.len() > 50;
        sources.truncate(50);
        let notes=json_rows(&tx,"SELECT jsonb_build_object('id',id,'title',title,'revision',revision) AS data FROM relay_agent_knowledge WHERE workspace=$1 AND agent=$2 AND chat IS NULL AND NOT forgotten ORDER BY created_at DESC,id DESC LIMIT 20",vec![w.into(),agent.into()]).await?;
        let active=query(&tx,"SELECT jsonb_build_object('id',id,'state',state) AS data FROM relay_agent_rehearsals WHERE workspace=$1 AND agent=$2 AND actor=$3 AND state IN ('queued','running')",vec![w.into(),agent.into(),user.id.clone().into()]).await?.map(|r|r.try_get::<Value>("","data")).transpose()?;
        tx.commit().await?;
        Ok(
            json!({"profile":super::rehearsal_context::profile_snapshot(&profile),"sources":sources,"more":more,"notes":notes,"active_sample":active}),
        )
    }
    pub(crate) async fn stop_rehearsal(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        id: &str,
    ) -> Result<Value> {
        uuid(id)?;
        let tx = self.workspace_tx(w).await?;
        super::agent_profiles::managed_access(&tx, &user.id, w, agent, true).await?;
        let data = record(&tx, w, id).await?;
        ensure!(
            data["actor"] == user.id && data["agent"] == agent,
            "Sample unavailable"
        );
        let changed=execute(&tx,"UPDATE relay_agent_rehearsals SET state='cancelled',claim=NULL,lease_until=0,prepared=NULL,detail='Stopped by requester',finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE workspace=$1 AND id=$2 AND state IN ('queued','running')",vec![w.into(),id.into()]).await?;
        if changed > 0 {
            super::executor_tasks::cancel(&tx, id).await?;
            audit(&tx, &user.id, Some(w), "agent_sample_stopped", id).await?;
        }
        // Terminating one's own sample does not require permission to disclose
        // withdrawn source text or its previous output back to the requester.
        let result=query(&tx,"SELECT jsonb_build_object('id',id,'state',state) AS data FROM relay_agent_rehearsals WHERE workspace=$1 AND id=$2",vec![w.into(),id.into()]).await?.unwrap().try_get::<Value>("","data")?;
        tx.commit().await?;
        Ok(result)
    }
}

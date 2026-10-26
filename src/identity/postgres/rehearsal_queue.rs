//! Small shared sample capacity with lease ownership and no automatic inference replay.
use super::*;
use crate::identity::chat::execution::Prepared;

#[derive(Clone)]
pub(crate) struct RehearsalClaim {
    pub workspace: String,
    pub id: String,
    pub agent: String,
    pub actor: String,
    pub token: String,
    pub prepared: Prepared,
}
impl RehearsalClaim {
    fn user(&self) -> User {
        User {
            id: self.actor.clone(),
            email: String::new(),
            display_name: None,
        }
    }
}
async fn invalidate(tx: &DatabaseTransaction, w: &str, id: &str) -> Result<()> {
    execute(tx,"UPDATE relay_agent_rehearsals SET state='invalidated',claim=NULL,lease_until=0,prepared=NULL,output=NULL,detail='Job access or selected knowledge changed. Start a new sample with current access.',finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE workspace=$1 AND id=$2 AND state IN ('queued','running')",vec![w.into(),id.into()]).await?;
    Ok(())
}
impl PgStore {
    pub(crate) async fn recover_rehearsals(&self) -> Result<()> {
        // Separate transaction: never retain a sample-row lock while later
        // acquiring a workspace lock used by user Stop/configuration writes.
        execute(&self.db,"UPDATE relay_agent_rehearsals SET state='interrupted',claim=NULL,lease_until=0,prepared=NULL,detail='The sample executor stopped or its lease expired. Start a new sample explicitly.',finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE state='running' AND lease_until<=floor(extract(epoch FROM clock_timestamp()))::bigint",vec![]).await?;
        Ok(())
    }
    pub(crate) async fn claim_rehearsal(&self) -> Result<Option<RehearsalClaim>> {
        let tx = self.db.begin().await?;
        // One controller-local setup helper already exists. Samples use at most
        // two live database leases across controller replicas, not two per replica.
        query(&tx, "SELECT pg_advisory_xact_lock(27110372)", vec![]).await?;
        let count: i64 = query(
            &tx,
            "SELECT count(*) AS n FROM relay_agent_rehearsals WHERE state='running'",
            vec![],
        )
        .await?
        .unwrap()
        .try_get("", "n")?;
        if count >= 2 {
            tx.commit().await?;
            return Ok(None);
        }
        let row=query(&tx,"SELECT workspace,id,agent,actor FROM relay_agent_rehearsals WHERE state='queued' ORDER BY created_ms,id LIMIT 1",vec![]).await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(None);
        };
        let w: String = row.try_get("", "workspace")?;
        let id: String = row.try_get("", "id")?;
        let agent: String = row.try_get("", "agent")?;
        let actor: String = row.try_get("", "actor")?;
        query(
            &tx,
            "SELECT id FROM relay_workspaces WHERE id=$1 FOR SHARE",
            vec![w.clone().into()],
        )
        .await?;
        let user = User {
            id: actor.clone(),
            email: String::new(),
            display_name: None,
        };
        let data = super::rehearsals::record(&tx, &w, &id).await?;
        if let Err(error) = super::rehearsal_context::readable(&tx, &user, &w, &agent, &data).await
        {
            if error.is::<sea_orm::DbErr>() {
                return Err(error);
            }
            invalidate(&tx, &w, &id).await?;
            tx.commit().await?;
            return Ok(None);
        }
        let prepared=query(&tx,"SELECT prepared FROM relay_agent_rehearsals WHERE workspace=$1 AND id=$2 AND state='queued'",vec![w.clone().into(),id.clone().into()]).await?;
        let Some(prepared) = prepared else {
            tx.commit().await?;
            return Ok(None);
        };
        let prepared: Prepared = serde_json::from_value(prepared.try_get("", "prepared")?)?;
        ensure!(
            prepared.version == 1
                && !prepared.control_tools
                && prepared.run_seconds == 45
                && prepared.workspace == w
                && prepared.agent == agent
                && prepared.reply == id,
            "Unsupported sample execution context"
        );
        let token = uuid::Uuid::new_v4().to_string();
        let changed=execute(&tx,"UPDATE relay_agent_rehearsals SET state='running',claim=$3,lease_until=floor(extract(epoch FROM clock_timestamp()))::bigint+15,started_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint,detail='Working on the draft-only sample' WHERE workspace=$1 AND id=$2 AND state='queued'",vec![w.clone().into(),id.clone().into(),token.clone().into()]).await?;
        tx.commit().await?;
        Ok((changed == 1).then_some(RehearsalClaim {
            workspace: w,
            id,
            agent,
            actor,
            token,
            prepared,
        }))
    }
    pub(crate) async fn heartbeat_rehearsal(&self, claim: &RehearsalClaim) -> Result<bool> {
        let tx = self.workspace_tx(&claim.workspace).await?;
        let data = super::rehearsals::record(&tx, &claim.workspace, &claim.id).await?;
        if let Err(error) = super::rehearsal_context::readable(
            &tx,
            &claim.user(),
            &claim.workspace,
            &claim.agent,
            &data,
        )
        .await
        {
            if error.is::<sea_orm::DbErr>() {
                return Err(error);
            }
            invalidate(&tx, &claim.workspace, &claim.id).await?;
            tx.commit().await?;
            return Ok(false);
        }
        let changed=execute(&tx,"UPDATE relay_agent_rehearsals SET lease_until=floor(extract(epoch FROM clock_timestamp()))::bigint+15 WHERE workspace=$1 AND id=$2 AND state='running' AND claim=$3 AND lease_until>floor(extract(epoch FROM clock_timestamp()))::bigint",vec![claim.workspace.clone().into(),claim.id.clone().into(),claim.token.clone().into()]).await?;
        tx.commit().await?;
        Ok(changed == 1)
    }
    pub(crate) async fn finish_rehearsal(
        &self,
        claim: &RehearsalClaim,
        output: Option<&str>,
        interrupted: bool,
    ) -> Result<bool> {
        ensure!(
            output.is_none_or(|s| !s.trim().is_empty() && s.len() <= 65536),
            "Sample output exceeds its supported size"
        );
        let tx = self.workspace_tx(&claim.workspace).await?;
        let data = super::rehearsals::record(&tx, &claim.workspace, &claim.id).await?;
        if let Err(error) = super::rehearsal_context::readable(
            &tx,
            &claim.user(),
            &claim.workspace,
            &claim.agent,
            &data,
        )
        .await
        {
            if error.is::<sea_orm::DbErr>() {
                return Err(error);
            }
            invalidate(&tx, &claim.workspace, &claim.id).await?;
            tx.commit().await?;
            return Ok(false);
        }
        let (state, detail) = if output.is_some() {
            (
                "completed",
                "Draft-only sample saved. No connected actions were executed.",
            )
        } else if interrupted {
            (
                "interrupted",
                "The sample executor stopped. Start a new sample explicitly.",
            )
        } else {
            (
                "failed",
                "The sample did not finish. Check its configuration or try a new sample explicitly.",
            )
        };
        let changed=execute(&tx,"UPDATE relay_agent_rehearsals SET state=$4,output=$5,detail=$6,claim=NULL,lease_until=0,prepared=NULL,finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE workspace=$1 AND id=$2 AND state='running' AND claim=$3 AND lease_until>floor(extract(epoch FROM clock_timestamp()))::bigint",vec![claim.workspace.clone().into(),claim.id.clone().into(),claim.token.clone().into(),state.into(),output.map(str::to_owned).into(),detail.into()]).await?;
        tx.commit().await?;
        Ok(changed == 1)
    }
}

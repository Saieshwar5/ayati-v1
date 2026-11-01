//! Agents' addresses on transports and which senders may write to them. Local
//! simulations and live transports share these records; live endpoints name
//! the adapter's server and hold no credentials.
use super::*;
use crate::identity::communications::model::{
    Binding, Endpoint, LiveEndpoint, SetState, Transport,
};
use serde_json::json;

impl PgStore {
    pub(crate) async fn communication_endpoint(
        &self,
        actor: &User,
        w: &str,
        input: &Endpoint,
    ) -> Result<Value> {
        ensure!(
            uuid::Uuid::parse_str(&input.id).is_ok(),
            "Invalid endpoint ID"
        );
        ensure!(
            input.transport != Transport::Chat,
            "Chat uses the existing authenticated conversation"
        );
        identifier(&input.address)?;
        let tx = self.workspace_tx(w).await?;
        super::access::owner(&tx, actor, w).await?;
        let profile =
            super::agent_profiles::managed_access(&tx, &actor.id, w, &input.agent, true).await?;
        ensure!(
            profile["revision"].as_i64().unwrap_or(0) > 0,
            "Save the managed agent profile before configuring communications"
        );
        if let Some(old) = endpoint(&tx, w, &input.id).await? {
            ensure!(
                old["agent"] == input.agent
                    && old["transport"] == input.transport.name()
                    && old["address"] == input.address,
                "Endpoint ID belongs to different configuration"
            );
            return Ok(old);
        }
        let count: i64 = query(
            &tx,
            "SELECT count(*) AS total FROM relay_communication_endpoints WHERE workspace=$1",
            vec![w.into()],
        )
        .await?
        .unwrap()
        .try_get("", "total")?;
        ensure!(count < 100, "Simulation endpoint limit reached");
        execute(&tx,"INSERT INTO relay_communication_endpoints(id,workspace,agent,transport,address) VALUES($1,$2,$3,$4,$5)",
            vec![input.id.clone().into(),w.into(),input.agent.clone().into(),input.transport.name().into(),input.address.clone().into()]).await?;
        audit(
            &tx,
            &actor.id,
            Some(w),
            "simulation_endpoint_created",
            &input.id,
        )
        .await?;
        let value = endpoint(&tx, w, &input.id).await?.unwrap();
        tx.commit().await?;
        Ok(value)
    }
    /// Saves an agent's address on a live transport. The caller has already
    /// asked the adapter to confirm the platform serves the address.
    pub(crate) async fn communication_live_endpoint(
        &self,
        actor: &User,
        w: &str,
        input: &LiveEndpoint,
    ) -> Result<Value> {
        ensure!(
            uuid::Uuid::parse_str(&input.id).is_ok(),
            "Invalid endpoint ID"
        );
        identifier(&input.address)?;
        identifier(&input.server)?;
        let tx = self.workspace_tx(w).await?;
        super::access::owner(&tx, actor, w).await?;
        let profile =
            super::super::agent_profiles::managed_access(&tx, &actor.id, w, &input.agent, true)
                .await?;
        ensure!(
            profile["revision"].as_i64().unwrap_or(0) > 0,
            "Save the managed agent profile before configuring communications"
        );
        if let Some(old) = endpoint(&tx, w, &input.id).await? {
            ensure!(
                old["agent"] == input.agent
                    && old["transport"] == input.transport
                    && old["server"] == input.server
                    && old["address"] == input.address,
                "Endpoint ID belongs to different configuration"
            );
            return Ok(old);
        }
        let count: i64 = query(
            &tx,
            "SELECT count(*) AS total FROM relay_communication_endpoints WHERE workspace=$1",
            vec![w.into()],
        )
        .await?
        .unwrap()
        .try_get("", "total")?;
        ensure!(count < 100, "Endpoint limit reached");
        ensure!(
            query(&tx, "SELECT id FROM relay_communication_endpoints WHERE workspace=$1 AND transport=$2 AND server=$3 AND address=$4",
                vec![w.into(), input.transport.clone().into(), input.server.clone().into(), input.address.clone().into()]).await?.is_none(),
            "This address already belongs to an agent"
        );
        execute(&tx,"INSERT INTO relay_communication_endpoints(id,workspace,agent,transport,server,address,simulated) VALUES($1,$2,$3,$4,$5,$6,FALSE)",
            vec![input.id.clone().into(),w.into(),input.agent.clone().into(),input.transport.clone().into(),input.server.clone().into(),input.address.clone().into()]).await?;
        audit(
            &tx,
            &actor.id,
            Some(w),
            "communication_endpoint_created",
            &input.id,
        )
        .await?;
        let value = endpoint(&tx, w, &input.id).await?.unwrap();
        tx.commit().await?;
        Ok(value)
    }
    /// Whether an endpoint is a local simulation, so each API only changes its own kind.
    pub(crate) async fn communication_simulated(&self, w: &str, id: &str) -> Result<bool> {
        let tx = self.read_tx(w).await?;
        let found = endpoint(&tx, w, id)
            .await?
            .context("Endpoint unavailable")?;
        tx.commit().await?;
        Ok(found["simulated"] == true)
    }
    pub(crate) async fn communication_binding(
        &self,
        actor: &User,
        w: &str,
        id: &str,
        input: &Binding,
    ) -> Result<Value> {
        identifier(&input.sender)?;
        let tx = self.workspace_tx(w).await?;
        super::access::owner(&tx, actor, w).await?;
        let e = endpoint(&tx, w, id)
            .await?
            .context("Endpoint unavailable")?;
        ensure!(
            input.sender != e["address"],
            "Cannot bind the agent's own sender address"
        );
        ensure!(
            super::chat::agent_access(&tx, &input.user, w, e["agent"].as_str().unwrap()).await?,
            "Person cannot use this agent"
        );
        if let Some(old)=query(&tx,"SELECT user_id FROM relay_communication_bindings WHERE workspace=$1 AND endpoint=$2 AND sender=$3",vec![w.into(),id.into(),input.sender.clone().into()]).await? {
            ensure!(old.try_get::<String>("","user_id")?==input.user,"A sender cannot be reassigned to another person");
        } else {
            let count:i64=query(&tx,"SELECT count(*) AS total FROM relay_communication_bindings WHERE workspace=$1",vec![w.into()]).await?.unwrap().try_get("","total")?;
            ensure!(count<1000,"Simulation binding limit reached");
            execute(&tx,"INSERT INTO relay_communication_bindings(workspace,endpoint,sender,user_id) VALUES($1,$2,$3,$4)",vec![w.into(),id.into(),input.sender.clone().into(),input.user.clone().into()]).await?;
            audit(&tx,&actor.id,Some(w),if e["simulated"]==true {"simulation_sender_bound"} else {"communication_sender_bound"},id).await?;
        }
        let result=json_rows(&tx,"SELECT to_jsonb(b) AS data FROM relay_communication_bindings b WHERE workspace=$1 AND endpoint=$2 AND sender=$3",vec![w.into(),id.into(),input.sender.clone().into()]).await?.remove(0);
        tx.commit().await?;
        Ok(result)
    }
    pub(crate) async fn communication_state(
        &self,
        actor: &User,
        w: &str,
        id: &str,
        sender: Option<&str>,
        input: &SetState,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        super::access::owner(&tx, actor, w).await?;
        let rows = if let Some(sender) = sender {
            execute(&tx,"UPDATE relay_communication_bindings SET enabled=$4,revision=revision+1 WHERE workspace=$1 AND endpoint=$2 AND sender=$3 AND revision=$5",vec![w.into(),id.into(),sender.into(),input.enabled.into(),input.revision.into()]).await?
        } else {
            execute(&tx,"UPDATE relay_communication_endpoints SET enabled=$3,revision=revision+1 WHERE workspace=$1 AND id=$2 AND revision=$4",vec![w.into(),id.into(),input.enabled.into(),input.revision.into()]).await?
        };
        ensure!(
            rows == 1,
            "Communication configuration changed; refresh before saving"
        );
        super::task_revocation::inaccessible(&tx, w).await?;
        execute(&tx,"UPDATE relay_communication_outbox o SET state='cancelled' WHERE workspace=$1 AND state IN ('queued','failed') AND NOT relay_communication_current(workspace,job->>'chat')",vec![w.into()]).await?;
        audit(&tx, &actor.id, Some(w), "communication_state_changed", id).await?;
        tx.commit().await?;
        Ok(json!({"enabled":input.enabled,"revision":input.revision+1}))
    }
    pub(crate) async fn communication_status(&self, actor: &User, w: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        super::access::owner(&tx, actor, w).await?;
        // Operator metadata only: ownership is not access to private message bodies.
        let endpoints=json_rows(&tx,"SELECT to_jsonb(e) AS data FROM relay_communication_endpoints e WHERE workspace=$1 ORDER BY id LIMIT 100",vec![w.into()]).await?;
        let inbox=json_rows(&tx,"SELECT jsonb_build_object('endpoint',endpoint,'event',event,'transport',transport,'status',status,'sequence',sequence) AS data FROM relay_communication_inbox WHERE workspace=$1 ORDER BY sequence DESC LIMIT 100",vec![w.into()]).await?;
        let outbox=json_rows(&tx,"SELECT jsonb_build_object('id',id,'endpoint',endpoint,'transport',transport,'state',state,'attempt',attempt,'provider_id',provider_id,'attempts',attempts,'sequence',sequence) AS data FROM relay_communication_outbox WHERE workspace=$1 ORDER BY sequence DESC LIMIT 100",vec![w.into()]).await?;
        tx.commit().await?;
        Ok(
            json!({"version":1,"endpoints":endpoints,"inbox":inbox,"outbox":outbox,
            "capabilities":{"chat":Transport::Chat.capabilities(),"email":Transport::Email.capabilities(),"whatsapp":Transport::Whatsapp.capabilities()}}),
        )
    }
}
pub(super) async fn endpoint(
    db: &impl ConnectionTrait,
    w: &str,
    id: &str,
) -> Result<Option<Value>> {
    Ok(json_rows(db,"SELECT to_jsonb(e) AS data FROM relay_communication_endpoints e WHERE workspace=$1 AND id=$2",vec![w.into(),id.into()]).await?.into_iter().next())
}
fn identifier(value: &str) -> Result<()> {
    ensure!(
        !value.trim().is_empty() && value.len() <= 320 && !value.chars().any(char::is_control),
        "Invalid communication address"
    );
    Ok(())
}

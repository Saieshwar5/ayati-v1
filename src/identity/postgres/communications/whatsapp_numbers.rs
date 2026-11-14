//! An agent's WhatsApp number: switched on with one action, from the pool of
//! numbers Cuckoo registered ahead of time, and released to rest when switched
//! off. The number becomes an ordinary live endpoint (`whatsapp`, `twilio`), so
//! the gateway's rooms, outbox and delivery rules apply unchanged.
use super::*;
use crate::identity::communications::whatsapp::settings::SERVER;
use serde_json::json;

const NOW: &str = "floor(extract(epoch FROM clock_timestamp()))::bigint";

/// No number is free right now; the operator needs to register more.
#[derive(Debug)]
pub(crate) struct NoFreeNumber;
impl std::fmt::Display for NoFreeNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("No WhatsApp number is free right now. Try again later.")
    }
}
impl std::error::Error for NoFreeNumber {}

/// What the operator offers: the registered numbers and how long released
/// numbers rest. `None` when this server offers no WhatsApp numbers.
pub(crate) struct Offer {
    pub numbers: Vec<String>,
    pub cooling_days: i64,
}

impl PgStore {
    /// Whether the agent has WhatsApp and its number. Anyone who may talk to
    /// the agent may see it: they need it to message the agent.
    pub(crate) async fn whatsapp_status(
        &self,
        actor: &User,
        w: &str,
        agent: &str,
        offered: bool,
    ) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let profile =
            super::super::agent_profiles::managed_access(&tx, &actor.id, w, agent, false).await?;
        let value = view(&tx, actor, w, agent, offered, &profile).await?;
        tx.commit().await?;
        Ok(value)
    }

    /// Gives the agent a WhatsApp number. Repeating it returns the same number;
    /// an agent switched on again while its number rests gets it back.
    pub(crate) async fn whatsapp_activate(
        &self,
        actor: &User,
        w: &str,
        agent: &str,
        offer: &Offer,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        let profile =
            super::super::agent_profiles::managed_access(&tx, &actor.id, w, agent, true).await?;
        ensure!(
            profile["revision"].as_i64().unwrap_or(0) > 0,
            "Save the agent before switching on WhatsApp"
        );
        let numbers = json!(offer.numbers);
        if assigned(&tx, w, agent).await?.is_none() {
            // The operator's file is the list of numbers; new ones join the pool.
            execute(&tx, &format!("INSERT INTO relay_whatsapp_numbers(number,state,changed) SELECT n,'ready',{NOW} FROM jsonb_array_elements_text($1::jsonb) n ON CONFLICT DO NOTHING"), vec![numbers.clone().into()]).await?;
            let resting = query(&tx, "SELECT number,endpoint FROM relay_whatsapp_numbers WHERE workspace=$1 AND agent=$2 AND state='cooling' AND number IN (SELECT jsonb_array_elements_text($3::jsonb)) ORDER BY changed DESC LIMIT 1 FOR UPDATE",
                vec![w.into(), agent.into(), numbers.clone().into()]).await?;
            if let Some(row) = resting {
                let (number, endpoint): (String, String) =
                    (row.try_get("", "number")?, row.try_get("", "endpoint")?);
                // A new generation: earlier conversations and sends stay closed.
                execute(&tx, "UPDATE relay_communication_endpoints SET enabled=TRUE,revision=revision+1 WHERE workspace=$1 AND id=$2", vec![w.into(), endpoint.into()]).await?;
                execute(&tx, &format!("UPDATE relay_whatsapp_numbers SET state='assigned',changed={NOW} WHERE number=$1"), vec![number.into()]).await?;
            } else {
                take(&tx, w, agent, &numbers).await?;
            }
            audit(&tx, &actor.id, Some(w), "whatsapp_activated", agent).await?;
        }
        let value = view(&tx, actor, w, agent, true, &profile).await?;
        tx.commit().await?;
        Ok(value)
    }

    /// Switches WhatsApp off for the agent. Replies not yet sent are cancelled
    /// and the number rests before another agent can have it.
    pub(crate) async fn whatsapp_deactivate(
        &self,
        actor: &User,
        w: &str,
        agent: &str,
        offer: &Offer,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        let profile =
            super::super::agent_profiles::managed_access(&tx, &actor.id, w, agent, true).await?;
        if let Some((number, endpoint)) = assigned(&tx, w, agent).await? {
            execute(&tx, "UPDATE relay_communication_endpoints SET enabled=FALSE,revision=revision+1 WHERE workspace=$1 AND id=$2", vec![w.into(), endpoint.into()]).await?;
            execute(&tx, &format!("UPDATE relay_whatsapp_numbers SET state='cooling',changed={NOW},ready_after={NOW}+$2*86400 WHERE number=$1"),
                vec![number.into(), offer.cooling_days.into()]).await?;
            fence(&tx, w).await?;
            audit(&tx, &actor.id, Some(w), "whatsapp_deactivated", agent).await?;
        }
        let value = view(&tx, actor, w, agent, true, &profile).await?;
        tx.commit().await?;
        Ok(value)
    }
}

/// Assigns a free number: never one this workspace used for another agent, so
/// one address never names two of its endpoints.
async fn take(tx: &DatabaseTransaction, w: &str, agent: &str, numbers: &Value) -> Result<()> {
    let free = query(tx, &format!("SELECT n.number FROM relay_whatsapp_numbers n
        WHERE n.number IN (SELECT jsonb_array_elements_text($2::jsonb))
          AND (n.state='ready' OR (n.state='cooling' AND n.ready_after<={NOW}))
          AND NOT EXISTS(SELECT 1 FROM relay_communication_endpoints e WHERE e.workspace=$1 AND e.transport='whatsapp' AND e.address=n.number)
        ORDER BY n.changed,n.number LIMIT 1 FOR UPDATE OF n SKIP LOCKED"), vec![w.into(), numbers.clone().into()]).await?;
    let Some(free) = free else {
        tracing::warn!("WhatsApp activation found no free number; register more numbers");
        return Err(NoFreeNumber.into());
    };
    let number: String = free.try_get("", "number")?;
    let count: i64 = query(
        tx,
        "SELECT count(*) AS total FROM relay_communication_endpoints WHERE workspace=$1",
        vec![w.into()],
    )
    .await?
    .context("Missing count")?
    .try_get("", "total")?;
    ensure!(count < 100, "Endpoint limit reached");
    let endpoint = uuid::Uuid::new_v4().to_string();
    execute(tx, "INSERT INTO relay_communication_endpoints(id,workspace,agent,transport,server,address,simulated) VALUES($1,$2,$3,'whatsapp',$4,$5,FALSE)",
        vec![endpoint.clone().into(), w.into(), agent.into(), SERVER.into(), number.clone().into()]).await?;
    execute(tx, &format!("UPDATE relay_whatsapp_numbers SET state='assigned',workspace=$2,agent=$3,endpoint=$4,changed={NOW},ready_after=0 WHERE number=$1"),
        vec![number.into(), w.into(), agent.into(), endpoint.into()]).await?;
    Ok(())
}

async fn assigned(
    db: &impl ConnectionTrait,
    w: &str,
    agent: &str,
) -> Result<Option<(String, String)>> {
    query(db, "SELECT number,endpoint FROM relay_whatsapp_numbers WHERE workspace=$1 AND agent=$2 AND state='assigned'", vec![w.into(), agent.into()])
        .await?
        .map(|r| Ok((r.try_get("", "number")?, r.try_get("", "endpoint")?)))
        .transpose()
}

/// Work and replies that lost their room's current generation stop.
pub(super) async fn fence(tx: &DatabaseTransaction, w: &str) -> Result<()> {
    super::super::task_revocation::inaccessible(tx, w).await?;
    super::access::cancel_stale(tx, w).await?;
    Ok(())
}

async fn view(
    db: &impl ConnectionTrait,
    actor: &User,
    w: &str,
    agent: &str,
    offered: bool,
    profile: &Value,
) -> Result<Value> {
    let found = json_rows(db, "SELECT jsonb_build_object('number',n.number,'enabled',e.enabled) AS data FROM relay_whatsapp_numbers n
        JOIN relay_communication_endpoints e ON e.workspace=n.workspace AND e.id=n.endpoint
        WHERE n.workspace=$1 AND n.agent=$2 AND n.state='assigned'", vec![w.into(), agent.into()]).await?.into_iter().next();
    let manage =
        profile["owner_id"] == actor.id.as_str() || role(db, w, &actor.id).await? == "Admin";
    Ok(match found {
        // An admin can pause the endpoint; the number stays the agent's.
        Some(n) => {
            json!({"offered":offered,"state":if n["enabled"]==true {"active"} else {"paused"},
            "number":n["number"],"can_manage":manage})
        }
        None => json!({"offered":offered,"state":"off","number":null,"can_manage":manage}),
    })
}

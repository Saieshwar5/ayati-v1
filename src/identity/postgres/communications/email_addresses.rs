//! An agent's email address: switched on with one action, made up on Cuckoo's
//! mail domain, and released when switched off. The address becomes an ordinary
//! live endpoint (`email`, `postmark`), so the gateway's rooms, outbox and
//! delivery rules apply unchanged. An address never serves another agent.
use super::*;
use crate::identity::communications::email::settings::SERVER;
use serde_json::json;

const NOW: &str = "floor(extract(epoch FROM clock_timestamp()))::bigint";
/// Lowercase letters and digits that are hard to confuse when read aloud.
const SUFFIX: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";

impl PgStore {
    /// Whether the agent has email and its address. Anyone who may talk to the
    /// agent may see it: they need it to write to the agent.
    pub(crate) async fn email_status(
        &self,
        actor: &User,
        w: &str,
        agent: &str,
        domain: Option<&str>,
    ) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let profile =
            super::super::agent_profiles::managed_access(&tx, &actor.id, w, agent, false).await?;
        let value = view(&tx, actor, w, agent, domain, &profile).await?;
        tx.commit().await?;
        Ok(value)
    }

    /// Gives the agent an address on `domain`. Repeating it returns the same
    /// address; an agent switched on again gets its earlier address back.
    pub(crate) async fn email_activate(
        &self,
        actor: &User,
        w: &str,
        agent: &str,
        domain: &str,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        let profile =
            super::super::agent_profiles::managed_access(&tx, &actor.id, w, agent, true).await?;
        ensure!(
            profile["revision"].as_i64().unwrap_or(0) > 0,
            "Save the agent before switching on email"
        );
        if assigned(&tx, w, agent).await?.is_none() {
            let released = query(&tx, "SELECT address,endpoint FROM relay_email_addresses WHERE workspace=$1 AND agent=$2 AND state='released' AND address LIKE $3 ORDER BY changed DESC LIMIT 1 FOR UPDATE",
                vec![w.into(), agent.into(), format!("%@{domain}").into()]).await?;
            if let Some(row) = released {
                let (address, endpoint): (String, String) =
                    (row.try_get("", "address")?, row.try_get("", "endpoint")?);
                // A new generation: earlier conversations and sends stay closed.
                execute(&tx, "UPDATE relay_communication_endpoints SET enabled=TRUE,revision=revision+1 WHERE workspace=$1 AND id=$2", vec![w.into(), endpoint.into()]).await?;
                execute(&tx, &format!("UPDATE relay_email_addresses SET state='assigned',changed={NOW} WHERE address=$1"), vec![address.into()]).await?;
            } else {
                create(
                    &tx,
                    w,
                    agent,
                    domain,
                    profile["name"].as_str().unwrap_or(""),
                )
                .await?;
            }
            audit(&tx, &actor.id, Some(w), "email_activated", agent).await?;
        }
        let value = view(&tx, actor, w, agent, Some(domain), &profile).await?;
        tx.commit().await?;
        Ok(value)
    }

    /// Switches email off for the agent. Replies not yet sent are cancelled and
    /// mail to the address is no longer read.
    pub(crate) async fn email_deactivate(
        &self,
        actor: &User,
        w: &str,
        agent: &str,
        domain: Option<&str>,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        let profile =
            super::super::agent_profiles::managed_access(&tx, &actor.id, w, agent, true).await?;
        if let Some((address, endpoint)) = assigned(&tx, w, agent).await? {
            execute(&tx, "UPDATE relay_communication_endpoints SET enabled=FALSE,revision=revision+1 WHERE workspace=$1 AND id=$2", vec![w.into(), endpoint.into()]).await?;
            execute(&tx, &format!("UPDATE relay_email_addresses SET state='released',changed={NOW} WHERE address=$1"), vec![address.into()]).await?;
            super::whatsapp_numbers::fence(&tx, w).await?;
            audit(&tx, &actor.id, Some(w), "email_deactivated", agent).await?;
        }
        let value = view(&tx, actor, w, agent, domain, &profile).await?;
        tx.commit().await?;
        Ok(value)
    }
}

/// The address's local part: the agent's name, then a short random suffix so
/// two agents named alike never collide and addresses can't be guessed.
fn local_part(name: &str) -> String {
    use ring::rand::{SecureRandom, SystemRandom};
    let mut slug = String::new();
    for c in name.to_ascii_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c);
        } else if !slug.is_empty() && !slug.ends_with('-') {
            slug.push('-');
        }
        if slug.len() >= 24 {
            break;
        }
    }
    let slug = slug.trim_end_matches('-');
    let slug = if slug.is_empty() { "agent" } else { slug };
    let mut bytes = [0u8; 5];
    SystemRandom::new()
        .fill(&mut bytes)
        .expect("OS randomness unavailable");
    let suffix: String = bytes
        .iter()
        .map(|b| SUFFIX[*b as usize % SUFFIX.len()] as char)
        .collect();
    format!("{slug}-{suffix}")
}

async fn create(
    tx: &DatabaseTransaction,
    w: &str,
    agent: &str,
    domain: &str,
    name: &str,
) -> Result<()> {
    let count: i64 = query(
        tx,
        "SELECT count(*) AS total FROM relay_communication_endpoints WHERE workspace=$1",
        vec![w.into()],
    )
    .await?
    .context("Missing count")?
    .try_get("", "total")?;
    ensure!(count < 100, "Endpoint limit reached");
    let mut address = None;
    for _ in 0..5 {
        let candidate = format!("{}@{domain}", local_part(name));
        let taken = query(tx, "SELECT 1 AS found FROM relay_email_addresses WHERE address=$1 UNION ALL SELECT 1 FROM relay_communication_endpoints WHERE transport='email' AND address=$1",
            vec![candidate.clone().into()]).await?.is_some();
        if !taken {
            address = Some(candidate);
            break;
        }
    }
    let address = address.context("Could not choose an email address. Try again.")?;
    let endpoint = uuid::Uuid::new_v4().to_string();
    execute(tx, "INSERT INTO relay_communication_endpoints(id,workspace,agent,transport,server,address,simulated) VALUES($1,$2,$3,'email',$4,$5,FALSE)",
        vec![endpoint.clone().into(), w.into(), agent.into(), SERVER.into(), address.clone().into()]).await?;
    execute(tx, &format!("INSERT INTO relay_email_addresses(address,workspace,agent,endpoint,state,changed) VALUES($1,$2,$3,$4,'assigned',{NOW})"),
        vec![address.into(), w.into(), agent.into(), endpoint.into()]).await?;
    Ok(())
}

async fn assigned(
    db: &impl ConnectionTrait,
    w: &str,
    agent: &str,
) -> Result<Option<(String, String)>> {
    query(db, "SELECT address,endpoint FROM relay_email_addresses WHERE workspace=$1 AND agent=$2 AND state='assigned'", vec![w.into(), agent.into()])
        .await?
        .map(|r| Ok((r.try_get("", "address")?, r.try_get("", "endpoint")?)))
        .transpose()
}

/// `offered` is whether this server has a mail domain at all. An address on a
/// domain the operator no longer serves shows as paused.
async fn view(
    db: &impl ConnectionTrait,
    actor: &User,
    w: &str,
    agent: &str,
    domain: Option<&str>,
    profile: &Value,
) -> Result<Value> {
    let found = json_rows(db, "SELECT jsonb_build_object('address',a.address,'enabled',e.enabled) AS data FROM relay_email_addresses a
        JOIN relay_communication_endpoints e ON e.workspace=a.workspace AND e.id=a.endpoint
        WHERE a.workspace=$1 AND a.agent=$2 AND a.state='assigned'", vec![w.into(), agent.into()]).await?.into_iter().next();
    let manage =
        profile["owner_id"] == actor.id.as_str() || role(db, w, &actor.id).await? == "Admin";
    let offered = domain.is_some();
    Ok(match found {
        Some(a) => {
            let served = domain.is_some_and(|d| {
                a["address"]
                    .as_str()
                    .is_some_and(|x| x.ends_with(&format!("@{d}")))
            });
            json!({"offered":offered,"state":if a["enabled"]==true && served {"active"} else {"paused"},
            "address":a["address"],"can_manage":manage})
        }
        None => json!({"offered":offered,"state":"off","address":null,"can_manage":manage}),
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn local_parts_come_from_the_name() {
        let part = super::local_part("Research & Planning — EU!");
        let (slug, suffix) = part.rsplit_once('-').unwrap();
        assert_eq!(slug, "research-planning-eu");
        assert_eq!(suffix.len(), 5);
        assert!(super::local_part("测试").starts_with("agent-"));
        assert!(super::local_part(&"x".repeat(80)).len() <= 30);
        assert_ne!(super::local_part("a"), super::local_part("a"));
    }
}

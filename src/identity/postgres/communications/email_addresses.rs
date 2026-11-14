//! An agent's email address: switched on with one action, made from the
//! agent's and workspace's names on the operator's mail domain, for example
//! `nova.acme@mail.cuckoo.onl`. The address becomes an ordinary live endpoint
//! (`email`, `ses`), so the gateway's rooms, outbox and delivery rules apply.
//! It is never given to another agent; switching off keeps it for this agent.
use super::*;
use crate::identity::communications::email::settings::SERVER;
use serde_json::json;

const NOW: &str = "floor(extract(epoch FROM clock_timestamp()))::bigint";
/// Local parts mail systems and people expect to reach a mail administrator.
const RESERVED: &[&str] = &[
    "abuse",
    "admin",
    "administrator",
    "hostmaster",
    "mailer-daemon",
    "noreply",
    "no-reply",
    "postmaster",
    "root",
    "security",
    "webmaster",
];

impl PgStore {
    /// Whether the agent has email and its address. Anyone who may talk to the
    /// agent may see it: they need it to write to the agent.
    pub(crate) async fn email_status(
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

    /// Gives the agent an address on `domain`. Repeating it returns the same
    /// address; switching on again brings back the agent's earlier address.
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
        match address(&tx, w, agent, Some(domain)).await? {
            Some(found) if found["enabled"] == true => {}
            Some(found) => {
                // A new generation: earlier requests and sends stay stopped.
                execute(&tx, "UPDATE relay_communication_endpoints SET enabled=TRUE,revision=revision+1 WHERE workspace=$1 AND id=$2",
                    vec![w.into(), found["endpoint"].as_str().unwrap_or_default().into()]).await?;
                audit(&tx, &actor.id, Some(w), "email_activated", agent).await?;
            }
            None => {
                create(&tx, w, agent, domain, &profile).await?;
                audit(&tx, &actor.id, Some(w), "email_activated", agent).await?;
            }
        }
        let value = view(&tx, actor, w, agent, true, &profile).await?;
        tx.commit().await?;
        Ok(value)
    }

    /// Switches the agent's email off. Mail to the address is held, and work
    /// and replies that came by email stop.
    pub(crate) async fn email_deactivate(
        &self,
        actor: &User,
        w: &str,
        agent: &str,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        let profile =
            super::super::agent_profiles::managed_access(&tx, &actor.id, w, agent, true).await?;
        for found in addresses(&tx, w, agent).await? {
            if found["enabled"] == true {
                execute(&tx, "UPDATE relay_communication_endpoints SET enabled=FALSE,revision=revision+1 WHERE workspace=$1 AND id=$2",
                    vec![w.into(), found["endpoint"].as_str().unwrap_or_default().into()]).await?;
                audit(&tx, &actor.id, Some(w), "email_deactivated", agent).await?;
            }
        }
        super::super::task_revocation::inaccessible(&tx, w).await?;
        super::access::cancel_stale(&tx, w).await?;
        let value = view(&tx, actor, w, agent, true, &profile).await?;
        tx.commit().await?;
        Ok(value)
    }
}

async fn addresses(db: &impl ConnectionTrait, w: &str, agent: &str) -> Result<Vec<Value>> {
    json_rows(db, "SELECT jsonb_build_object('address',a.address,'domain',a.domain,'endpoint',a.endpoint,'enabled',e.enabled) AS data
        FROM relay_email_addresses a JOIN relay_communication_endpoints e ON e.workspace=a.workspace AND e.id=a.endpoint
        WHERE a.workspace=$1 AND a.agent=$2 ORDER BY a.created DESC", vec![w.into(), agent.into()]).await
}

/// The agent's address on `domain`, or its newest one on any domain.
async fn address(
    db: &impl ConnectionTrait,
    w: &str,
    agent: &str,
    domain: Option<&str>,
) -> Result<Option<Value>> {
    Ok(addresses(db, w, agent)
        .await?
        .into_iter()
        .find(|a| domain.is_none_or(|d| a["domain"] == d)))
}

async fn create(
    tx: &DatabaseTransaction,
    w: &str,
    agent: &str,
    domain: &str,
    profile: &Value,
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
    let team: String = query(
        tx,
        "SELECT name FROM relay_workspaces WHERE id=$1",
        vec![w.into()],
    )
    .await?
    .context("Workspace unavailable")?
    .try_get("", "name")?;
    let (name, team) = (
        slug(profile["name"].as_str().unwrap_or_default(), "agent"),
        slug(&team, "team"),
    );
    // Lock the domain's addresses so two activations never pick the same one.
    execute(
        tx,
        "SELECT pg_advisory_xact_lock(hashtext('relay_email_addresses:' || $1))",
        vec![domain.into()],
    )
    .await?;
    let mut chosen = None;
    for n in 1..=50 {
        // A second agent of the same name: `nova-2.acme`.
        let local = if n == 1 {
            format!("{name}.{team}")
        } else {
            format!("{name}-{n}.{team}")
        };
        let candidate = format!("{local}@{domain}");
        if RESERVED.contains(&local.as_str()) {
            continue;
        }
        let taken = query(
            tx,
            "SELECT 1 AS found FROM relay_email_addresses WHERE address=$1",
            vec![candidate.clone().into()],
        )
        .await?
        .is_some();
        if !taken {
            chosen = Some(candidate);
            break;
        }
    }
    let address =
        chosen.context("No free email address for this agent; rename it and try again")?;
    let endpoint = uuid::Uuid::new_v4().to_string();
    execute(tx, "INSERT INTO relay_communication_endpoints(id,workspace,agent,transport,server,address,simulated) VALUES($1,$2,$3,'email',$4,$5,FALSE)",
        vec![endpoint.clone().into(), w.into(), agent.into(), SERVER.into(), address.clone().into()]).await?;
    execute(tx, &format!("INSERT INTO relay_email_addresses(address,domain,workspace,agent,endpoint,created) VALUES($1,$2,$3,$4,$5,{NOW})"),
        vec![address.into(), domain.into(), w.into(), agent.into(), endpoint.into()]).await?;
    Ok(())
}

/// A name as an address part: lowercase letters and digits, words joined by
/// hyphens, at most 24 characters. `fallback` when nothing is left.
pub(crate) fn slug(name: &str, fallback: &str) -> String {
    let mut out = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if c.is_alphanumeric() {
            // Letters outside ASCII are left out rather than splitting the word.
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    let out: String = out.trim_end_matches('-').chars().take(24).collect();
    let out = out.trim_end_matches('-');
    if out.is_empty() {
        fallback.to_owned()
    } else {
        out.to_owned()
    }
}

async fn view(
    db: &impl ConnectionTrait,
    actor: &User,
    w: &str,
    agent: &str,
    offered: bool,
    profile: &Value,
) -> Result<Value> {
    let found = address(db, w, agent, None).await?;
    let manage =
        profile["owner_id"] == actor.id.as_str() || role(db, w, &actor.id).await? == "Admin";
    Ok(match found {
        Some(a) if a["enabled"] == true => {
            json!({"offered":offered,"state":"active","address":a["address"],"can_manage":manage})
        }
        // Switched off: the address is kept for this agent.
        Some(a) => {
            json!({"offered":offered,"state":"off","address":a["address"],"can_manage":manage})
        }
        None => json!({"offered":offered,"state":"off","address":null,"can_manage":manage}),
    })
}

#[cfg(test)]
mod tests {
    use super::slug;

    #[test]
    fn names_become_plain_address_parts() {
        assert_eq!(slug("Nova", "agent"), "nova");
        assert_eq!(slug("  HR Helper (India) ", "agent"), "hr-helper-india");
        assert_eq!(slug("Ünïcode Støre", "team"), "ncode-stre");
        assert_eq!(slug("日本", "team"), "team");
        assert_eq!(
            slug("a very long agent name that keeps going", "agent"),
            "a-very-long-agent-name-t"
        );
    }
}

//! A workspace's Telegram and its agents' bots. An admin connects Telegram once
//! and becomes the workspace's Telegram owner. Switching Telegram on for an
//! agent asks that owner to create the agent's bot with one tap in Telegram;
//! Cuckoo's manager bot is told when it exists and the bot becomes an ordinary
//! live endpoint (`telegram`, address the bot's ID), so the gateway's rooms,
//! outbox and delivery rules apply unchanged.
use super::*;
use crate::identity::communications::telegram::settings::SERVER;
use serde_json::json;

const NOW: &str = "floor(extract(epoch FROM clock_timestamp()))::bigint";
const LEASE_SECONDS: i64 = 30;
/// How long a request to create a bot waits for the owner's tap.
const REQUEST_SECONDS: i64 = 7 * 86400;

/// The workspace has no Telegram owner yet, or the owner's Telegram is gone.
#[derive(Debug)]
pub(crate) struct NotConnected;
impl std::fmt::Display for NotConnected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("A workspace admin needs to connect Telegram in Settings first.")
    }
}
impl std::error::Error for NotConnected {}

/// A request for the owner to create an agent's bot, sent to them in Telegram
/// when someone else switched Telegram on.
pub(crate) struct OwnerAsk {
    pub telegram_id: i64,
    pub request_id: i32,
    pub name: String,
    pub username: String,
    pub text: String,
}
pub(crate) struct Activation {
    pub view: Value,
    pub ask: Option<OwnerAsk>,
}
/// What a newly managed bot turned out to be.
#[derive(Debug, PartialEq)]
pub(crate) enum Created {
    Attached {
        bot: i64,
        agent: String,
        workspace: String,
    },
    /// A bot Cuckoo already serves whose token or owner changed.
    Known,
    Unmatched,
}
/// One agent bot whose updates this controller may read now.
pub(crate) struct Polled {
    pub workspace: String,
    pub bot: i64,
    pub next: i64,
}

impl PgStore {
    /// Whether the workspace's Telegram is connected and by whom.
    pub(crate) async fn telegram_workspace(
        &self,
        actor: &User,
        w: &str,
        offered: bool,
    ) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let admin = role(&tx, w, &actor.id).await? == "Admin";
        let owner = json_rows(&tx, "SELECT jsonb_build_object('id',t.owner,'name',COALESCE(u.display_name,u.email),'linked',EXISTS(SELECT 1 FROM relay_user_telegram m WHERE m.user_id=t.owner)) AS data
            FROM relay_telegram_workspaces t JOIN relay_users u ON u.id=t.owner WHERE t.workspace=$1", vec![w.into()]).await?.into_iter().next();
        tx.commit().await?;
        Ok(match owner {
            Some(o) => json!({"offered":offered,"connected":o["linked"]==true,"owner":o["name"],
                "is_owner":o["id"]==actor.id.as_str(),"can_manage":admin}),
            None => {
                json!({"offered":offered,"connected":false,"owner":null,"is_owner":false,"can_manage":admin})
            }
        })
    }

    /// A Start code that links the admin's Telegram and makes them the
    /// workspace's Telegram owner. Admins only.
    pub(crate) async fn telegram_connect_code(&self, actor: &User, w: &str) -> Result<String> {
        ensure!(
            self.role(w, &actor.id).await? == "Admin",
            "Only workspace admins can connect Telegram"
        );
        super::telegram_people::new_code(&self.db, &actor.id, Some(w)).await
    }

    /// The agent's Telegram. Anyone who may talk to the agent may see it: they
    /// need the bot to message the agent.
    pub(crate) async fn telegram_status(
        &self,
        actor: &User,
        w: &str,
        agent: &str,
        offered: bool,
        manager: &str,
    ) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let profile =
            super::super::agent_profiles::managed_access(&tx, &actor.id, w, agent, false).await?;
        let value = view(&tx, actor, w, agent, offered, manager, &profile).await?;
        tx.commit().await?;
        Ok(value)
    }

    /// Switches Telegram on for the agent: its bot comes back if it has one,
    /// otherwise the workspace's Telegram owner is asked to create it.
    pub(crate) async fn telegram_activate(
        &self,
        actor: &User,
        w: &str,
        agent: &str,
        manager: &str,
    ) -> Result<Activation> {
        let tx = self.workspace_tx(w).await?;
        let profile =
            super::super::agent_profiles::managed_access(&tx, &actor.id, w, agent, true).await?;
        ensure!(
            profile["revision"].as_i64().unwrap_or(0) > 0,
            "Save the agent before switching on Telegram"
        );
        let owner = query(&tx, "SELECT t.owner,m.telegram_id FROM relay_telegram_workspaces t JOIN relay_user_telegram m ON m.user_id=t.owner WHERE t.workspace=$1", vec![w.into()]).await?;
        let Some(owner) = owner else {
            return Err(NotConnected.into());
        };
        let (owner_user, owner_telegram): (String, i64) = (
            owner.try_get("", "owner")?,
            owner.try_get("", "telegram_id")?,
        );
        let row = json_rows(&tx, "SELECT to_jsonb(b) AS data FROM relay_telegram_bots b WHERE workspace=$1 AND agent=$2 FOR UPDATE", vec![w.into(), agent.into()]).await?.into_iter().next();
        let mut ask = None;
        match row.as_ref().and_then(|r| r["state"].as_str()) {
            Some("active") => {}
            Some("off") => {
                let endpoint = row.as_ref().unwrap()["endpoint"]
                    .as_str()
                    .context("Missing endpoint")?
                    .to_owned();
                // A new generation: earlier conversations and sends stay closed.
                execute(&tx, "UPDATE relay_communication_endpoints SET enabled=TRUE,revision=revision+1 WHERE workspace=$1 AND id=$2", vec![w.into(), endpoint.into()]).await?;
                execute(&tx, &format!("UPDATE relay_telegram_bots SET state='active',changed={NOW} WHERE workspace=$1 AND agent=$2"), vec![w.into(), agent.into()]).await?;
                audit(&tx, &actor.id, Some(w), "telegram_activated", agent).await?;
            }
            requested => {
                let (request_id, username) = match (requested, &row) {
                    (Some("requested"), Some(r)) => (
                        r["request_id"].as_i64().context("Missing request")? as i32,
                        r["suggested"]
                            .as_str()
                            .context("Missing request")?
                            .to_owned(),
                    ),
                    _ => (
                        request_number(),
                        suggested_username(profile["name"].as_str().unwrap_or("agent")),
                    ),
                };
                execute(&tx, &format!("INSERT INTO relay_telegram_bots(workspace,agent,state,requested_by,request_id,suggested,requested,changed) VALUES($1,$2,'requested',$3,$4,$5,{NOW},{NOW})
                    ON CONFLICT(workspace,agent) DO UPDATE SET requested_by=EXCLUDED.requested_by,requested=EXCLUDED.requested,changed=EXCLUDED.changed"),
                    vec![w.into(), agent.into(), owner_telegram.into(), request_id.into(), username.clone().into()]).await?;
                if requested.is_none() {
                    audit(&tx, &actor.id, Some(w), "telegram_requested", agent).await?;
                }
                if owner_user != actor.id {
                    let (name, asker) = names(&tx, w, agent, &actor.id).await?;
                    ask = Some(OwnerAsk {
                        telegram_id: owner_telegram,
                        request_id,
                        text: format!(
                            "{asker} switched on Telegram for the agent {}. Tap the button below to create its bot.",
                            profile["name"].as_str().unwrap_or("agent")
                        ),
                        name,
                        username,
                    });
                }
            }
        }
        let value = view(&tx, actor, w, agent, true, manager, &profile).await?;
        tx.commit().await?;
        Ok(Activation { view: value, ask })
    }

    /// Switches Telegram off for the agent. Replies not yet sent are cancelled;
    /// the bot stays the agent's for when Telegram is switched on again.
    pub(crate) async fn telegram_deactivate(
        &self,
        actor: &User,
        w: &str,
        agent: &str,
        manager: &str,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        let profile =
            super::super::agent_profiles::managed_access(&tx, &actor.id, w, agent, true).await?;
        let row = query(&tx, "SELECT state,endpoint FROM relay_telegram_bots WHERE workspace=$1 AND agent=$2 FOR UPDATE", vec![w.into(), agent.into()]).await?;
        if let Some(row) = row {
            let state: String = row.try_get("", "state")?;
            if state == "requested" {
                execute(
                    &tx,
                    "DELETE FROM relay_telegram_bots WHERE workspace=$1 AND agent=$2",
                    vec![w.into(), agent.into()],
                )
                .await?;
            } else if state == "active" {
                let endpoint: String = row.try_get("", "endpoint")?;
                execute(&tx, "UPDATE relay_communication_endpoints SET enabled=FALSE,revision=revision+1 WHERE workspace=$1 AND id=$2", vec![w.into(), endpoint.into()]).await?;
                execute(&tx, &format!("UPDATE relay_telegram_bots SET state='off',changed={NOW} WHERE workspace=$1 AND agent=$2"), vec![w.into(), agent.into()]).await?;
                super::whatsapp_numbers::fence(&tx, w).await?;
            }
            audit(&tx, &actor.id, Some(w), "telegram_deactivated", agent).await?;
        }
        let value = view(&tx, actor, w, agent, true, manager, &profile).await?;
        tx.commit().await?;
        Ok(value)
    }

    /// The manager bot was told a bot was created for it to manage, or that a
    /// managed bot's token or owner changed. A new bot is matched to the open
    /// request of the Telegram account that created it: the one with the same
    /// username, else its latest.
    pub(crate) async fn telegram_bot_created(
        &self,
        creator: i64,
        bot: i64,
        username: Option<&str>,
    ) -> Result<Created> {
        if query(
            &self.db,
            "SELECT 1 AS found FROM relay_telegram_bots WHERE bot_id=$1",
            vec![bot.into()],
        )
        .await?
        .is_some()
        {
            return Ok(Created::Known);
        }
        let found = query(&self.db, &format!("SELECT workspace,agent FROM relay_telegram_bots WHERE state='requested' AND requested_by=$1 AND requested>{NOW}-{REQUEST_SECONDS}
            ORDER BY suggested IS NOT DISTINCT FROM $2 DESC,requested DESC LIMIT 1"), vec![creator.into(), username.map(str::to_lowercase).into()]).await?;
        let Some(found) = found else {
            return Ok(Created::Unmatched);
        };
        let (w, agent): (String, String) =
            (found.try_get("", "workspace")?, found.try_get("", "agent")?);
        let tx = self.workspace_tx(&w).await?;
        let still = query(&tx, "SELECT 1 AS found FROM relay_telegram_bots WHERE workspace=$1 AND agent=$2 AND state='requested' AND requested_by=$3 FOR UPDATE", vec![w.clone().into(), agent.clone().into(), creator.into()]).await?;
        if still.is_none() {
            return Ok(Created::Unmatched);
        }
        let count: i64 = query(
            &tx,
            "SELECT count(*) AS total FROM relay_communication_endpoints WHERE workspace=$1",
            vec![w.clone().into()],
        )
        .await?
        .context("Missing count")?
        .try_get("", "total")?;
        ensure!(count < 100, "Endpoint limit reached");
        let endpoint = uuid::Uuid::new_v4().to_string();
        execute(&tx, "INSERT INTO relay_communication_endpoints(id,workspace,agent,transport,server,address,simulated) VALUES($1,$2,$3,'telegram',$4,$5,FALSE)",
            vec![endpoint.clone().into(), w.clone().into(), agent.clone().into(), SERVER.into(), bot.to_string().into()]).await?;
        execute(&tx, &format!("UPDATE relay_telegram_bots SET state='active',bot_id=$3,username=$4,endpoint=$5,changed={NOW} WHERE workspace=$1 AND agent=$2"),
            vec![w.clone().into(), agent.clone().into(), bot.into(), username.into(), endpoint.into()]).await?;
        tx.commit().await?;
        Ok(Created::Attached {
            bot,
            agent,
            workspace: w,
        })
    }

    /// The agent's name and description for its bot's Telegram profile.
    pub(crate) async fn telegram_bot_profile(
        &self,
        w: &str,
        agent: &str,
    ) -> Result<(String, String)> {
        let row = query(&self.db, "SELECT a.name,a.role,ws.name AS workspace FROM relay_shared_agents a JOIN relay_workspaces ws ON ws.id=a.workspace WHERE a.workspace=$1 AND a.id=$2", vec![w.into(), agent.into()])
            .await?.context("Agent unavailable")?;
        let (name, role, workspace): (String, String, String) = (
            row.try_get("", "name")?,
            row.try_get("", "role")?,
            row.try_get("", "workspace")?,
        );
        let description: String = format!("{name}, {role}, for members of {workspace} on Cuckoo. Connect Telegram in your Cuckoo profile to talk to it.").chars().take(500).collect();
        Ok((name, description))
    }

    /// Takes the manager bot's read lease; `None` while another controller has it.
    pub(crate) async fn telegram_manager_claim(&self) -> Result<Option<i64>> {
        query(&self.db, &format!("UPDATE relay_telegram_manager SET lease_until={NOW}+{LEASE_SECONDS} WHERE id=1 AND lease_until<={NOW} RETURNING next_update"), vec![])
            .await?.map(|r| r.try_get("", "next_update").map_err(Into::into)).transpose()
    }
    pub(crate) async fn telegram_manager_release(&self, next: i64) -> Result<()> {
        execute(&self.db, "UPDATE relay_telegram_manager SET next_update=GREATEST(next_update,$1),lease_until=0 WHERE id=1", vec![next.into()]).await?;
        Ok(())
    }

    /// Takes the read lease of every active bot no other controller is reading.
    pub(crate) async fn telegram_bot_claims(&self) -> Result<Vec<Polled>> {
        let found = rows(&self.db, &format!("UPDATE relay_telegram_bots b SET lease_until={NOW}+{LEASE_SECONDS}
            WHERE (b.workspace,b.agent) IN (SELECT t.workspace,t.agent FROM relay_telegram_bots t JOIN relay_communication_endpoints e ON e.workspace=t.workspace AND e.id=t.endpoint
                WHERE t.state='active' AND e.enabled AND t.lease_until<={NOW} ORDER BY t.changed LIMIT 200 FOR UPDATE OF t SKIP LOCKED)
            RETURNING b.workspace,b.bot_id,b.next_update"), vec![]).await?;
        found
            .iter()
            .map(|r| {
                Ok(Polled {
                    workspace: r.try_get("", "workspace")?,
                    bot: r.try_get("", "bot_id")?,
                    next: r.try_get("", "next_update")?,
                })
            })
            .collect()
    }
    pub(crate) async fn telegram_bot_release(&self, bot: i64, next: i64) -> Result<()> {
        execute(&self.db, "UPDATE relay_telegram_bots SET next_update=GREATEST(next_update,$2),lease_until=0 WHERE bot_id=$1", vec![bot.into(), next.into()]).await?;
        Ok(())
    }
}

/// The bot's suggested display name and the asker's name.
async fn names(
    db: &impl ConnectionTrait,
    w: &str,
    agent: &str,
    asker: &str,
) -> Result<(String, String)> {
    let row = query(db, "SELECT a.name AS agent,ws.name AS workspace,(SELECT COALESCE(u.display_name,u.email) FROM relay_users u WHERE u.id=$3) AS asker
        FROM relay_shared_agents a JOIN relay_workspaces ws ON ws.id=a.workspace WHERE a.workspace=$1 AND a.id=$2", vec![w.into(), agent.into(), asker.into()]).await?.context("Agent unavailable")?;
    let (agent, workspace, asker): (String, String, Option<String>) = (
        row.try_get("", "agent")?,
        row.try_get("", "workspace")?,
        row.try_get("", "asker")?,
    );
    Ok((
        bot_name(&agent, &workspace),
        asker.unwrap_or_else(|| "Someone".into()),
    ))
}

/// A bot's display name: at most 64 characters.
pub(crate) fn bot_name(agent: &str, workspace: &str) -> String {
    format!("{agent} · {workspace}").chars().take(64).collect()
}

/// A username for Telegram to suggest: the agent's name in Latin letters and
/// digits, a random part so it is free, and `bot`.
pub(crate) fn suggested_username(agent: &str) -> String {
    use ring::rand::{SecureRandom, SystemRandom};
    let mut base = String::new();
    for c in agent.chars().map(|c| c.to_ascii_lowercase()) {
        if c.is_ascii_alphanumeric() {
            base.push(c);
        } else if !base.ends_with('_') {
            base.push('_');
        }
    }
    let mut base: String = base.trim_matches('_').chars().take(16).collect();
    base = base.trim_end_matches('_').to_owned();
    if !base.starts_with(|c: char| c.is_ascii_alphabetic()) {
        base = format!("agent{base}").chars().take(16).collect();
    }
    let mut bytes = [0u8; 5];
    SystemRandom::new()
        .fill(&mut bytes)
        .expect("OS randomness unavailable");
    const ALPHABET: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    let suffix: String = bytes
        .iter()
        .map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char)
        .collect();
    format!("{base}_{suffix}_bot")
}

fn request_number() -> i32 {
    use ring::rand::{SecureRandom, SystemRandom};
    let mut bytes = [0u8; 4];
    SystemRandom::new()
        .fill(&mut bytes)
        .expect("OS randomness unavailable");
    (u32::from_be_bytes(bytes) >> 1).max(1) as i32
}

async fn view(
    db: &impl ConnectionTrait,
    actor: &User,
    w: &str,
    agent: &str,
    offered: bool,
    manager: &str,
    profile: &Value,
) -> Result<Value> {
    let row = json_rows(db, "SELECT jsonb_build_object('state',b.state,'username',b.username,'suggested',b.suggested,'enabled',e.enabled) AS data FROM relay_telegram_bots b
        LEFT JOIN relay_communication_endpoints e ON e.workspace=b.workspace AND e.id=b.endpoint WHERE b.workspace=$1 AND b.agent=$2", vec![w.into(), agent.into()]).await?.into_iter().next();
    let owner = query(db, "SELECT t.owner,EXISTS(SELECT 1 FROM relay_user_telegram m WHERE m.user_id=t.owner) AS linked FROM relay_telegram_workspaces t WHERE t.workspace=$1", vec![w.into()]).await?;
    let (connected, is_owner) = match owner {
        Some(o) => (
            o.try_get::<bool>("", "linked")?,
            o.try_get::<String>("", "owner")? == actor.id,
        ),
        None => (false, false),
    };
    let manage =
        profile["owner_id"] == actor.id.as_str() || role(db, w, &actor.id).await? == "Admin";
    let mut value = json!({"offered":offered,"connected":connected,"state":"off","username":null,"link":null,
        "create_link":null,"is_owner":is_owner,"can_manage":manage});
    if let Some(row) = row {
        let state = match (row["state"].as_str(), row["enabled"].as_bool()) {
            (Some("active"), Some(false)) => "paused",
            (Some(state), _) => state,
            _ => "off",
        };
        value["state"] = json!(state);
        if let Some(username) = row["username"].as_str().filter(|_| state != "off") {
            value["username"] = json!(username);
            value["link"] = json!(format!("https://t.me/{username}"));
        }
        if state == "requested" && is_owner {
            let workspace: String = query(
                db,
                "SELECT name FROM relay_workspaces WHERE id=$1",
                vec![w.into()],
            )
            .await?
            .context("Workspace unavailable")?
            .try_get("", "name")?;
            let name = bot_name(profile["name"].as_str().unwrap_or("Agent"), &workspace);
            let mut url = reqwest::Url::parse(&format!(
                "https://t.me/newbot/{manager}/{}",
                row["suggested"].as_str().unwrap_or_default()
            ))?;
            url.query_pairs_mut().append_pair("name", &name);
            value["create_link"] = json!(url.to_string());
        }
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn suggested_usernames_are_valid_telegram_bot_usernames() {
        for agent in [
            "Nova",
            "Accounts & Billing assistant for everyone",
            "42",
            "नोवा",
            "",
        ] {
            let name = suggested_username(agent);
            assert!(
                crate::identity::communications::telegram::settings::bot_username(&name),
                "{agent}: {name}"
            );
            assert!(
                name.starts_with(|c: char| c.is_ascii_alphabetic()),
                "{name}"
            );
        }
        assert!(suggested_username("Nova").starts_with("nova_"));
        assert_ne!(suggested_username("Nova"), suggested_username("Nova"));
        assert_eq!(bot_name("Nova", "Sharma Stores"), "Nova · Sharma Stores");
        assert_eq!(bot_name(&"n".repeat(70), "x").chars().count(), 64);
        assert!(request_number() > 0);
    }
}

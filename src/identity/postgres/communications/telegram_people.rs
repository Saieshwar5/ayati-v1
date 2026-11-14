//! People's Telegram accounts and messages to agents' bots. A person links
//! their account once by pressing Start on Cuckoo's manager bot with a code
//! from their signed-in session; after that the numeric Telegram ID on every
//! message says who wrote it. Anyone else is held without agent work, as for
//! any stranger on the gateway.
use super::*;
use crate::identity::{communications::model::Message, provider::digest};
use serde_json::json;

const NOW: &str = "floor(extract(epoch FROM clock_timestamp()))::bigint";
const CODE_SECONDS: i64 = 15 * 60;
const CODE_CHARS: usize = 24;
const ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZabcdefghjkmnpqrstuvwxyz23456789";

/// The Telegram account a message or Start came from, as Telegram reported it.
pub(crate) struct From {
    pub id: i64,
    pub username: Option<String>,
}

/// What a Start code did.
#[derive(Debug, PartialEq)]
pub(crate) enum Linked {
    /// Linked to this person.
    Person(String),
    /// Linked, and this person is now the workspace's Telegram owner.
    Owner {
        person: String,
        workspace: String,
    },
    Invalid,
    Taken,
}

/// What became of one message to an agent's bot.
#[derive(Debug, PartialEq)]
pub(crate) enum Outcome {
    Admitted,
    /// Refused before agent work: a stranger, or no access to this agent.
    Held {
        workspace: String,
    },
    Duplicate,
    Ignored,
}

/// One private text message to an agent's bot.
pub(crate) struct Incoming {
    pub bot: i64,
    pub update: i64,
    pub from: i64,
    pub chat: i64,
    pub text: String,
}

/// A Start code: `c`, then letters and digits, as Telegram allows in a link.
pub(crate) fn start_code(payload: &str) -> Option<&str> {
    let code = payload.trim();
    (code.len() == CODE_CHARS + 1
        && code.starts_with('c')
        && code[1..].bytes().all(|b| ALPHABET.contains(&b)))
    .then_some(code)
}

pub(super) async fn new_code(
    db: &impl ConnectionTrait,
    user: &str,
    w: Option<&str>,
) -> Result<String> {
    use ring::rand::{SecureRandom, SystemRandom};
    let mut bytes = [0u8; CODE_CHARS];
    SystemRandom::new()
        .fill(&mut bytes)
        .expect("OS randomness unavailable");
    let code: String = std::iter::once('c')
        .chain(
            bytes
                .iter()
                .map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char),
        )
        .collect();
    execute(db, &format!("INSERT INTO relay_telegram_link_codes(digest,user_id,workspace,expires) VALUES($1,$2,$3,{NOW}+{CODE_SECONDS})
        ON CONFLICT(user_id) DO UPDATE SET digest=EXCLUDED.digest,workspace=EXCLUDED.workspace,expires=EXCLUDED.expires"),
        vec![digest(&code).into(), user.into(), w.into()]).await?;
    Ok(code)
}

async fn person_name(db: &impl ConnectionTrait, user: &str) -> Result<String> {
    Ok(query(
        db,
        "SELECT COALESCE(display_name,email) AS name FROM relay_users WHERE id=$1",
        vec![user.into()],
    )
    .await?
    .context("Person unavailable")?
    .try_get("", "name")?)
}

impl PgStore {
    /// The person's linked Telegram account, if any.
    pub(crate) async fn telegram_link_status(&self, actor: &User) -> Result<Value> {
        let found = query(
            &self.db,
            "SELECT username FROM relay_user_telegram WHERE user_id=$1",
            vec![actor.id.clone().into()],
        )
        .await?;
        Ok(match found {
            Some(r) => {
                json!({"linked":true,"username":r.try_get::<Option<String>>("","username")?})
            }
            None => json!({"linked":false,"username":null}),
        })
    }

    /// A new Start code for the person. It replaces any earlier code.
    pub(crate) async fn telegram_link_code(&self, actor: &User) -> Result<String> {
        new_code(&self.db, &actor.id, None).await
    }

    /// Unlinks the person's Telegram. Their Telegram conversations close and
    /// replies not yet sent are cancelled. The account stays theirs.
    pub(crate) async fn telegram_unlink(&self, actor: &User) -> Result<Value> {
        let removed = query(
            &self.db,
            "DELETE FROM relay_user_telegram WHERE user_id=$1 RETURNING telegram_id",
            vec![actor.id.clone().into()],
        )
        .await?;
        if let Some(row) = removed {
            let telegram: i64 = row.try_get("", "telegram_id")?;
            self.telegram_bindings(&actor.id, telegram, false).await?;
            audit(&self.db, &actor.id, None, "telegram_unlinked", &actor.id).await?;
        }
        Ok(json!({"linked":false,"username":null}))
    }

    /// Switches the person's Telegram senders on or off in every workspace.
    /// Each change is a new generation: old conversations never come back.
    async fn telegram_bindings(&self, user: &str, telegram: i64, enabled: bool) -> Result<()> {
        let sender = telegram.to_string();
        let found = rows(&self.db, "SELECT DISTINCT b.workspace FROM relay_communication_bindings b JOIN relay_communication_endpoints e ON e.workspace=b.workspace AND e.id=b.endpoint
            WHERE b.user_id=$1 AND b.sender=$2 AND b.enabled<>$3 AND e.transport='telegram' AND NOT e.simulated", vec![user.into(), sender.clone().into(), enabled.into()]).await?;
        for row in found {
            let w: String = row.try_get("", "workspace")?;
            let tx = self.workspace_tx(&w).await?;
            execute(&tx, "UPDATE relay_communication_bindings b SET enabled=$4,revision=b.revision+1 FROM relay_communication_endpoints e
                WHERE e.workspace=b.workspace AND e.id=b.endpoint AND b.workspace=$1 AND b.user_id=$2 AND b.sender=$3 AND b.enabled<>$4 AND e.transport='telegram' AND NOT e.simulated",
                vec![w.clone().into(), user.into(), sender.clone().into(), enabled.into()]).await?;
            if !enabled {
                super::whatsapp_numbers::fence(&tx, &w).await?;
            }
            tx.commit().await?;
        }
        Ok(())
    }

    /// A Start code pressed on Cuckoo's manager bot.
    pub(crate) async fn telegram_start(&self, code: &str, from: &From) -> Result<Linked> {
        let tx = self.db.begin().await?;
        let found = query(&tx, &format!("DELETE FROM relay_telegram_link_codes WHERE digest=$1 AND expires>{NOW} RETURNING user_id,workspace"), vec![digest(code).into()]).await?;
        let Some(found) = found else {
            tx.commit().await?;
            return Ok(Linked::Invalid);
        };
        let (user, w): (String, Option<String>) = (
            found.try_get("", "user_id")?,
            found.try_get("", "workspace")?,
        );
        // An account someone else linked stays theirs: senders are never reassigned.
        let other = query(&tx, "SELECT 1 AS found FROM relay_telegram_accounts WHERE telegram_id=$1 AND user_id<>$2
            UNION ALL SELECT 1 FROM relay_user_telegram WHERE telegram_id=$1 AND user_id<>$2 LIMIT 1", vec![from.id.into(), user.clone().into()]).await?;
        if other.is_some() {
            tx.commit().await?;
            return Ok(Linked::Taken);
        }
        let previous: Option<i64> = query(
            &tx,
            "SELECT telegram_id FROM relay_user_telegram WHERE user_id=$1",
            vec![user.clone().into()],
        )
        .await?
        .map(|r| r.try_get("", "telegram_id"))
        .transpose()?;
        let username = from
            .username
            .clone()
            .filter(|u| u.len() <= 64 && u.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
        execute(&tx, &format!("INSERT INTO relay_user_telegram(user_id,telegram_id,username,linked) VALUES($1,$2,$3,{NOW})
            ON CONFLICT(user_id) DO UPDATE SET telegram_id=EXCLUDED.telegram_id,username=EXCLUDED.username,linked=EXCLUDED.linked"),
            vec![user.clone().into(), from.id.into(), username.into()]).await?;
        execute(&tx, "INSERT INTO relay_telegram_accounts(telegram_id,user_id) VALUES($1,$2) ON CONFLICT DO NOTHING", vec![from.id.into(), user.clone().into()]).await?;
        audit(&tx, &user, None, "telegram_linked", &user).await?;
        let person = person_name(&tx, &user).await?;
        let mut linked = Linked::Person(person.clone());
        // Only someone who is still an admin becomes the Telegram owner.
        if let Some(w) = w
            && role(&tx, &w, &user).await.ok().as_deref() == Some("Admin")
        {
            execute(&tx, &format!("INSERT INTO relay_telegram_workspaces(workspace,owner,connected) VALUES($1,$2,{NOW})
                ON CONFLICT(workspace) DO UPDATE SET owner=EXCLUDED.owner,connected=EXCLUDED.connected"), vec![w.clone().into(), user.clone().into()]).await?;
            audit(&tx, &user, Some(&w), "telegram_connected", &w).await?;
            let workspace: String = query(
                &tx,
                "SELECT name FROM relay_workspaces WHERE id=$1",
                vec![w.into()],
            )
            .await?
            .context("Workspace unavailable")?
            .try_get("", "name")?;
            linked = Linked::Owner { person, workspace };
        }
        tx.commit().await?;
        if let Some(previous) = previous.filter(|p| *p != from.id) {
            self.telegram_bindings(&user, previous, false).await?;
        }
        // Senders closed by an earlier unlink open again, as a new generation.
        self.telegram_bindings(&user, from.id, true).await?;
        Ok(linked)
    }

    /// Who pressed Start on an agent's bot: their name and the agent's, when
    /// they may talk to it; `None` for anyone the bot must refuse.
    pub(crate) async fn telegram_greeting(
        &self,
        bot: i64,
        from: i64,
    ) -> Result<(Option<(String, String)>, Option<String>)> {
        let found = query(&self.db, "SELECT b.workspace,b.agent,ws.name AS workspace_name FROM relay_telegram_bots b JOIN relay_workspaces ws ON ws.id=b.workspace WHERE b.bot_id=$1 AND b.state='active'", vec![bot.into()]).await?;
        let Some(found) = found else {
            return Ok((None, None));
        };
        let (w, agent, name): (String, String, String) = (
            found.try_get("", "workspace")?,
            found.try_get("", "agent")?,
            found.try_get("", "workspace_name")?,
        );
        let tx = self.read_tx(&w).await?;
        let user: Option<String> = query(&tx, "SELECT u.user_id FROM relay_user_telegram u JOIN relay_members m ON m.user_id=u.user_id AND m.workspace=$2 WHERE u.telegram_id=$1", vec![from.into(), w.clone().into()])
            .await?.map(|r| r.try_get("", "user_id")).transpose()?;
        let mut welcome = None;
        if let Some(user) = user
            && super::super::chat::agent_access(&tx, &user, &w, &agent).await?
        {
            let agent_name: String = query(
                &tx,
                "SELECT name FROM relay_shared_agents WHERE workspace=$1 AND id=$2",
                vec![w.clone().into(), agent.into()],
            )
            .await?
            .context("Agent unavailable")?
            .try_get("", "name")?;
            welcome = Some((person_name(&tx, &user).await?, agent_name));
        }
        tx.commit().await?;
        Ok((welcome, Some(name)))
    }

    /// One private text message to an agent's bot.
    pub(crate) async fn telegram_incoming(&self, input: &Incoming) -> Result<Outcome> {
        let found = query(&self.db, "SELECT b.workspace,b.endpoint,ws.name FROM relay_telegram_bots b JOIN relay_workspaces ws ON ws.id=b.workspace WHERE b.bot_id=$1 AND b.state='active'", vec![input.bot.into()]).await?;
        let Some(found) = found else {
            return Ok(Outcome::Ignored);
        };
        let (w, id, workspace): (String, String, String) = (
            found.try_get("", "workspace")?,
            found.try_get("", "endpoint")?,
            found.try_get("", "name")?,
        );
        let sender = input.from.to_string();
        let tx = self.workspace_tx(&w).await?;
        let endpoint = super::endpoints::endpoint(&tx, &w, &id)
            .await?
            .context("Endpoint unavailable")?;
        let owner: Option<String> = query(
            &tx,
            "SELECT user_id FROM relay_user_telegram WHERE telegram_id=$1",
            vec![input.from.into()],
        )
        .await?
        .map(|r| r.try_get("", "user_id"))
        .transpose()?;
        let binding = query(&tx, "SELECT user_id,enabled FROM relay_communication_bindings WHERE workspace=$1 AND endpoint=$2 AND sender=$3",
            vec![w.clone().into(), id.clone().into(), sender.clone().into()]).await?;
        match (binding, &owner) {
            // The account no longer belongs to the person this sender was bound to.
            (Some(b), _)
                if b.try_get::<bool>("", "enabled")?
                    && owner.as_deref() != Some(b.try_get::<String>("", "user_id")?.as_str()) =>
            {
                execute(&tx, "UPDATE relay_communication_bindings SET enabled=FALSE,revision=revision+1 WHERE workspace=$1 AND endpoint=$2 AND sender=$3",
                    vec![w.clone().into(), id.clone().into(), sender.clone().into()]).await?;
                super::whatsapp_numbers::fence(&tx, &w).await?;
            }
            (None, Some(user)) => {
                let member = query(
                    &tx,
                    "SELECT 1 AS found FROM relay_members WHERE workspace=$1 AND user_id=$2",
                    vec![w.clone().into(), user.clone().into()],
                )
                .await?
                .is_some();
                if member {
                    execute(&tx, "INSERT INTO relay_communication_bindings(workspace,endpoint,sender,user_id) VALUES($1,$2,$3,$4)",
                        vec![w.clone().into(), id.clone().into(), sender.clone().into(), user.clone().into()]).await?;
                }
            }
            _ => {}
        }
        // One conversation per person and agent: a private chat with the bot.
        let message = Message {
            event: format!("update:{}", input.update),
            sender,
            recipient: input.bot.to_string(),
            conversation: input.chat.to_string(),
            text: input.text.clone(),
            attachments: vec![],
            automated: false,
            thread: None,
        };
        message.validate()?;
        let saved = self.receive(&tx, &w, &endpoint, &message).await?;
        tx.commit().await?;
        Ok(
            match (saved["duplicate"] == true, saved["status"].as_str()) {
                (true, _) => Outcome::Duplicate,
                (false, Some("admitted")) => Outcome::Admitted,
                (false, Some("held")) => Outcome::Held { workspace },
                _ => Outcome::Ignored,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_codes_have_one_shape() {
        let code = format!("c{}", "A".repeat(CODE_CHARS));
        assert_eq!(start_code(&code), Some(code.as_str()));
        for bad in [
            format!("c{}", "A".repeat(CODE_CHARS - 1)),
            format!("x{}", "A".repeat(CODE_CHARS)),
            format!("c{}0", "A".repeat(CODE_CHARS - 1)),
            format!("c{}_", "A".repeat(CODE_CHARS - 1)),
            String::new(),
        ] {
            assert_eq!(start_code(&bad), None, "{bad}");
        }
    }
}

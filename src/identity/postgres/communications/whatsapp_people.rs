//! People's WhatsApp numbers and incoming WhatsApp messages. A person links
//! their number by sending a short code from it to any Cuckoo WhatsApp number;
//! after that, the number a message comes from says who wrote it. Someone with
//! no linked number can't reach an agent on WhatsApp: their message is held
//! without agent work, as for any stranger on the gateway.
use super::*;
use crate::identity::{
    communications::{model::Message, whatsapp::settings::SERVER},
    provider::digest,
};
use serde_json::json;

const NOW: &str = "floor(extract(epoch FROM clock_timestamp()))::bigint";
const CODE_SECONDS: i64 = 15 * 60;
/// No 0/O, 1/I/L: the code is typed on a phone.
const ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";

/// One message Twilio delivered to a Cuckoo number. The webhook has already
/// checked Twilio's signature.
pub(crate) struct Incoming {
    pub event: String,
    pub from: String,
    pub to: String,
    pub text: String,
}

/// What became of it, and the short fixed reply (if any) Twilio should send
/// back. A fixed reply is never agent work.
#[derive(Debug, PartialEq)]
pub(crate) enum Outcome {
    Linked,
    Admitted,
    Held,
    Duplicate,
    Ignored,
}
impl Outcome {
    pub fn reply(&self, link_problem: Option<&str>) -> Option<String> {
        match self {
            Self::Linked => Some(
                "Your WhatsApp number is now linked to Cuckoo. You can message your agents here."
                    .into(),
            ),
            Self::Held => Some(
                "This agent can't take messages from this number. Link your WhatsApp number in your Cuckoo profile, or ask your workspace admin for access."
                    .into(),
            ),
            _ => link_problem.map(str::to_owned),
        }
    }
}

/// `LINK-` and six symbols, as typed or pasted; case and spaces don't matter.
fn link_code(text: &str) -> Option<String> {
    let normal: String = text
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_uppercase();
    let symbols = normal.strip_prefix("LINK-")?;
    (symbols.len() == 6 && symbols.bytes().all(|b| ALPHABET.contains(&b))).then_some(normal)
}

impl PgStore {
    /// The person's linked WhatsApp number, if any.
    pub(crate) async fn mobile_status(&self, actor: &User) -> Result<Value> {
        let found = query(
            &self.db,
            "SELECT number,verified FROM relay_user_mobiles WHERE user_id=$1",
            vec![actor.id.clone().into()],
        )
        .await?;
        Ok(match found {
            Some(r) => json!({"number":r.try_get::<String>("","number")?,
                "verified":r.try_get::<i64>("","verified")?}),
            None => json!({"number":null}),
        })
    }

    /// A new code to send from WhatsApp. It replaces any earlier code.
    pub(crate) async fn mobile_link_code(&self, actor: &User) -> Result<Value> {
        let code = new_code();
        execute(&self.db, &format!("INSERT INTO relay_mobile_link_codes(digest,user_id,expires) VALUES($1,$2,{NOW}+{CODE_SECONDS})
            ON CONFLICT(user_id) DO UPDATE SET digest=EXCLUDED.digest,expires=EXCLUDED.expires"),
            vec![digest(&code).into(), actor.id.clone().into()]).await?;
        Ok(json!({"code":code,"expires_in":CODE_SECONDS}))
    }

    /// Unlinks the person's number. Their WhatsApp conversations close and
    /// replies not yet sent are cancelled.
    pub(crate) async fn mobile_unlink(&self, actor: &User) -> Result<Value> {
        let removed = query(
            &self.db,
            "DELETE FROM relay_user_mobiles WHERE user_id=$1 RETURNING number",
            vec![actor.id.clone().into()],
        )
        .await?;
        if let Some(row) = removed {
            let number: String = row.try_get("", "number")?;
            self.mobile_bindings(&actor.id, &number, false).await?;
            audit(&self.db, &actor.id, None, "mobile_unlinked", &actor.id).await?;
        }
        Ok(json!({"number":null}))
    }

    /// Switches the person's WhatsApp senders on or off in every workspace.
    /// Each change is a new generation: old conversations never come back.
    async fn mobile_bindings(&self, user: &str, number: &str, enabled: bool) -> Result<()> {
        let found = rows(&self.db, "SELECT DISTINCT b.workspace FROM relay_communication_bindings b JOIN relay_communication_endpoints e ON e.workspace=b.workspace AND e.id=b.endpoint
            WHERE b.user_id=$1 AND b.sender=$2 AND b.enabled<>$3 AND e.transport='whatsapp' AND NOT e.simulated", vec![user.into(), number.into(), enabled.into()]).await?;
        for row in found {
            let w: String = row.try_get("", "workspace")?;
            let tx = self.workspace_tx(&w).await?;
            execute(&tx, "UPDATE relay_communication_bindings b SET enabled=$4,revision=b.revision+1 FROM relay_communication_endpoints e
                WHERE e.workspace=b.workspace AND e.id=b.endpoint AND b.workspace=$1 AND b.user_id=$2 AND b.sender=$3 AND b.enabled<>$4 AND e.transport='whatsapp' AND NOT e.simulated",
                vec![w.clone().into(), user.into(), number.into(), enabled.into()]).await?;
            if !enabled {
                super::whatsapp_numbers::fence(&tx, &w).await?;
            }
            tx.commit().await?;
        }
        Ok(())
    }

    /// A message to one of Cuckoo's WhatsApp numbers. Returns what happened and
    /// a problem with a link code to tell the sender, if there was one.
    pub(crate) async fn whatsapp_incoming(
        &self,
        input: &Incoming,
    ) -> Result<(Outcome, Option<&'static str>)> {
        if let Some(code) = link_code(&input.text) {
            return self.link(input, &code).await;
        }
        let pool = query(&self.db, "SELECT workspace,endpoint FROM relay_whatsapp_numbers WHERE number=$1 AND state='assigned'", vec![input.to.clone().into()]).await?;
        let Some(pool) = pool else {
            return Ok((Outcome::Ignored, None));
        };
        let (w, id): (String, String) = (
            pool.try_get("", "workspace")?,
            pool.try_get("", "endpoint")?,
        );
        let tx = self.workspace_tx(&w).await?;
        let endpoint = super::endpoints::endpoint(&tx, &w, &id)
            .await?
            .context("Endpoint unavailable")?;
        let owner: Option<String> = query(
            &tx,
            "SELECT user_id FROM relay_user_mobiles WHERE number=$1",
            vec![input.from.clone().into()],
        )
        .await?
        .map(|r| r.try_get("", "user_id"))
        .transpose()?;
        let binding = query(&tx, "SELECT user_id,enabled FROM relay_communication_bindings WHERE workspace=$1 AND endpoint=$2 AND sender=$3",
            vec![w.clone().into(), id.clone().into(), input.from.clone().into()]).await?;
        match (binding, &owner) {
            // The number no longer belongs to the person this sender was bound to.
            (Some(b), _)
                if b.try_get::<bool>("", "enabled")?
                    && owner.as_deref() != Some(b.try_get::<String>("", "user_id")?.as_str()) =>
            {
                execute(&tx, "UPDATE relay_communication_bindings SET enabled=FALSE,revision=revision+1 WHERE workspace=$1 AND endpoint=$2 AND sender=$3",
                    vec![w.clone().into(), id.clone().into(), input.from.clone().into()]).await?;
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
                        vec![w.clone().into(), id.clone().into(), input.from.clone().into(), user.clone().into()]).await?;
                }
            }
            _ => {}
        }
        // One conversation per person and agent: WhatsApp has no threads.
        let message = Message {
            event: input.event.clone(),
            sender: input.from.clone(),
            recipient: input.to.clone(),
            conversation: input.from.clone(),
            text: input.text.clone(),
            attachments: vec![],
            automated: false,
            thread: None,
        };
        message.validate()?;
        let saved = self.receive(&tx, &w, &endpoint, &message).await?;
        tx.commit().await?;
        Ok((
            match (saved["duplicate"] == true, saved["status"].as_str()) {
                (true, _) => Outcome::Duplicate,
                (false, Some("admitted")) => Outcome::Admitted,
                (false, Some("held")) => Outcome::Held,
                _ => Outcome::Ignored,
            },
            None,
        ))
    }

    async fn link(&self, input: &Incoming, code: &str) -> Result<(Outcome, Option<&'static str>)> {
        const EXPIRED: &str =
            "This code is invalid or has expired. Get a new code in your Cuckoo profile.";
        const TAKEN: &str = "This WhatsApp number is already linked to another Cuckoo account.";
        let tx = self.db.begin().await?;
        let found = query(&tx, &format!("DELETE FROM relay_mobile_link_codes WHERE digest=$1 AND expires>{NOW} RETURNING user_id"), vec![digest(code).into()]).await?;
        let Some(found) = found else {
            return Ok((Outcome::Ignored, Some(EXPIRED)));
        };
        let user: String = found.try_get("", "user_id")?;
        // A number someone else used stays theirs: senders are never reassigned.
        let other = query(&tx, "SELECT 1 AS found FROM relay_user_mobiles WHERE number=$1 AND user_id<>$2
            UNION ALL SELECT 1 FROM relay_communication_bindings WHERE sender=$1 AND user_id<>$2 LIMIT 1", vec![input.from.clone().into(), user.clone().into()]).await?;
        if other.is_some() {
            tx.commit().await?;
            return Ok((Outcome::Ignored, Some(TAKEN)));
        }
        let previous: Option<String> = query(
            &tx,
            "SELECT number FROM relay_user_mobiles WHERE user_id=$1",
            vec![user.clone().into()],
        )
        .await?
        .map(|r| r.try_get("", "number"))
        .transpose()?;
        execute(
            &tx,
            &format!(
                "INSERT INTO relay_user_mobiles(user_id,number,verified) VALUES($1,$2,{NOW})
            ON CONFLICT(user_id) DO UPDATE SET number=EXCLUDED.number,verified=EXCLUDED.verified"
            ),
            vec![user.clone().into(), input.from.clone().into()],
        )
        .await?;
        audit(&tx, &user, None, "mobile_linked", &user).await?;
        tx.commit().await?;
        if let Some(previous) = previous.filter(|p| *p != input.from) {
            self.mobile_bindings(&user, &previous, false).await?;
        }
        // Senders closed by an earlier unlink open again, as a new generation.
        self.mobile_bindings(&user, &input.from, true).await?;
        Ok((Outcome::Linked, None))
    }

    /// Twilio's delivery update for one attempt in `w`.
    pub(crate) async fn whatsapp_receipt(&self, w: &str, key: &str, delivered: bool) -> Result<()> {
        let source = crate::identity::communications::adapters::Source {
            workspace: w.into(),
            transport: "whatsapp".into(),
            server: SERVER.into(),
        };
        self.transport_receipt(&source, key, delivered).await
    }
}

fn new_code() -> String {
    use ring::rand::{SecureRandom, SystemRandom};
    let mut bytes = [0u8; 6];
    SystemRandom::new()
        .fill(&mut bytes)
        .expect("OS randomness unavailable");
    let symbols: String = bytes
        .iter()
        .map(|b| ALPHABET[*b as usize % ALPHABET.len()] as char)
        .collect();
    format!("LINK-{symbols}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_codes_are_recognised_however_they_are_typed() {
        let code = new_code();
        assert_eq!(link_code(&code).as_deref(), Some(code.as_str()));
        assert_eq!(link_code(" link-abc234 ").as_deref(), Some("LINK-ABC234"));
        for text in [
            "LINK-ABC23",
            "LINK-ABC2340",
            "LINK-ABO234",
            "Hello LINK-ABC234",
            "LINKABC234",
        ] {
            assert_eq!(link_code(text), None, "{text}");
        }
    }
}

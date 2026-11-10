//! Incoming email. The address it was delivered to decides the workspace and
//! agent; the sender's address, once its own domain vouched for the message,
//! is matched to a workspace member's sign-in email. Anyone else is held
//! without agent work. Mail that is shared with other people, automated or
//! not vouched for is recorded as ignored and never answered.
use super::*;
use crate::identity::communications::{
    adapters::{EmailReply, Source},
    email::{inbound::Parsed, settings::SERVER},
    model::Message,
};
use serde_json::json;

const ATTACHMENTS: &str = "\n\n[This email had attachments, which can't be read yet.]";

/// What became of an incoming email. Email never gets a fixed reply: a
/// sender's address can be forged, and answering forged mail is spam.
#[derive(Debug, PartialEq)]
pub(crate) enum Outcome {
    Admitted,
    Held,
    Duplicate,
    Ignored,
}

impl PgStore {
    pub(crate) async fn email_incoming(&self, input: &Parsed) -> Result<Outcome> {
        let found = query(&self.db, "SELECT workspace,endpoint FROM relay_email_addresses WHERE address=$1 AND state='assigned'", vec![input.to.clone().into()]).await?;
        let Some(found) = found else {
            return Ok(Outcome::Ignored);
        };
        let (w, id): (String, String) = (
            found.try_get("", "workspace")?,
            found.try_get("", "endpoint")?,
        );
        let text = (input.authenticated && !input.others && !input.automated)
            .then(|| crate::identity::communications::email::inbound::admit(input).ok())
            .flatten();
        let readable = text.is_some();
        let text = match text {
            Some(text) if input.attachments => format!("{text}{ATTACHMENTS}"),
            Some(text) => text.to_owned(),
            // Not kept for ignored mail; it only fills the envelope's digest.
            None => format!("(not read: {})", input.event),
        };
        let tx = self.workspace_tx(&w).await?;
        let endpoint = super::endpoints::endpoint(&tx, &w, &id)
            .await?
            .context("Endpoint unavailable")?;
        if readable {
            bind(&tx, &w, &id, &input.from).await?;
        }
        let message = Message {
            event: input.event.clone(),
            sender: input.from.clone(),
            recipient: input.to.clone(),
            conversation: input.root.clone(),
            text,
            attachments: vec![],
            automated: !readable,
            thread: None,
        };
        message.validate()?;
        let saved = self.receive(&tx, &w, &endpoint, &message).await?;
        let outcome = match (saved["duplicate"] == true, saved["status"].as_str()) {
            (true, _) => Outcome::Duplicate,
            (false, Some("admitted")) => Outcome::Admitted,
            (false, Some("held")) => Outcome::Held,
            _ => Outcome::Ignored,
        };
        if outcome == Outcome::Admitted {
            execute(&tx, "INSERT INTO relay_email_messages(workspace,endpoint,event,message_id,reference_ids,subject) VALUES($1,$2,$3,$4,$5,$6)",
                vec![w.clone().into(), id.into(), input.event.clone().into(), input.message_id.clone().into(), json!(input.references).into(), input.subject.clone().into()]).await?;
        }
        tx.commit().await?;
        Ok(outcome)
    }

    /// Postmark's delivery or bounce event for one attempt in `w`.
    pub(crate) async fn email_receipt(&self, w: &str, key: &str, delivered: bool) -> Result<()> {
        let source = Source {
            workspace: w.into(),
            transport: "email".into(),
            server: SERVER.into(),
        };
        self.transport_receipt(&source, key, delivered).await
    }
}

/// Binds the sender to the one member whose sign-in email it is. A binding
/// whose address no longer belongs to its person is switched off.
async fn bind(tx: &DatabaseTransaction, w: &str, id: &str, from: &str) -> Result<()> {
    let owners = rows(tx, "SELECT u.id FROM relay_users u JOIN relay_members m ON m.user_id=u.id AND m.workspace=$1 WHERE lower(u.email)=$2 LIMIT 2",
        vec![w.into(), from.into()]).await?;
    // Two members with one sign-in email: nobody can be told apart.
    let owner: Option<String> = match owners.as_slice() {
        [one] => Some(one.try_get("", "id")?),
        _ => None,
    };
    let binding = query(tx, "SELECT user_id,enabled FROM relay_communication_bindings WHERE workspace=$1 AND endpoint=$2 AND sender=$3",
        vec![w.into(), id.into(), from.into()]).await?;
    match (binding, owner) {
        (Some(b), owner)
            if b.try_get::<bool>("", "enabled")?
                && owner.as_deref() != Some(b.try_get::<String>("", "user_id")?.as_str()) =>
        {
            execute(tx, "UPDATE relay_communication_bindings SET enabled=FALSE,revision=revision+1 WHERE workspace=$1 AND endpoint=$2 AND sender=$3",
                vec![w.into(), id.into(), from.into()]).await?;
            super::whatsapp_numbers::fence(tx, w).await?;
        }
        (None, Some(user)) => {
            execute(tx, "INSERT INTO relay_communication_bindings(workspace,endpoint,sender,user_id) VALUES($1,$2,$3,$4)",
                vec![w.into(), id.into(), from.into(), user.into()]).await?;
        }
        _ => {}
    }
    Ok(())
}

/// How the reply to inbox event `event` joins its thread, and the agent's name.
pub(super) async fn reply(
    tx: &DatabaseTransaction,
    w: &str,
    endpoint: &Value,
    event: &str,
) -> Result<Option<EmailReply>> {
    let id = endpoint["id"].as_str().context("Endpoint unavailable")?;
    let agent = endpoint["agent"].as_str().context("Endpoint unavailable")?;
    let found = query(tx, "SELECT m.message_id,m.reference_ids::text AS reference_ids,m.subject,COALESCE(a.name,'') AS name FROM relay_email_messages m
        LEFT JOIN relay_shared_agents a ON a.workspace=m.workspace AND a.id=$4
        WHERE m.workspace=$1 AND m.endpoint=$2 AND m.event=$3", vec![w.into(), id.into(), event.into(), agent.into()]).await?;
    found
        .map(|row| {
            Ok(EmailReply {
                in_reply_to: row.try_get("", "message_id")?,
                references: serde_json::from_str(&row.try_get::<String>("", "reference_ids")?)?,
                subject: row.try_get("", "subject")?,
                from_name: row.try_get("", "name")?,
            })
        })
        .transpose()
}

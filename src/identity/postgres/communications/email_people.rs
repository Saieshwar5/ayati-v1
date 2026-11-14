//! A mail to an agent's address. The address it was sent to decides the
//! workspace and agent; who wrote is the `From` address only when SES says
//! DMARC passed, and only if it is a workspace member's sign-in address. Then
//! the mail joins the person's direct room with the agent, in the room thread
//! of its mail thread. Anyone else is held without agent work.
use super::*;
use crate::identity::communications::{email::inbound::Mail, model::Message};
use serde_json::json;

/// One mail for one of Cuckoo's addresses, as the queue reader saw it.
pub(crate) struct Incoming<'a> {
    /// SES's ID for the mail: unique, so redeliveries are duplicates.
    pub event: String,
    /// The envelope recipient on Cuckoo's domain.
    pub to: String,
    /// SES's DMARC verdict passed.
    pub dmarc: bool,
    pub mail: &'a Mail,
}

#[derive(Debug, PartialEq)]
pub(crate) enum Outcome {
    Admitted,
    Held,
    Ignored,
    Duplicate,
}

impl PgStore {
    pub(crate) async fn email_incoming(&self, input: &Incoming<'_>) -> Result<Outcome> {
        let to = without_tag(&input.to);
        let found = query(
            &self.db,
            "SELECT workspace,endpoint FROM relay_email_addresses WHERE address=$1",
            vec![to.clone().into()],
        )
        .await?;
        let Some(found) = found else {
            return Ok(Outcome::Ignored);
        };
        let (w, id): (String, String) = (
            found.try_get("", "workspace")?,
            found.try_get("", "endpoint")?,
        );
        let tx = self.workspace_tx(&w).await?;
        let endpoint = super::endpoints::endpoint(&tx, &w, &id)
            .await?
            .context("Endpoint unavailable")?;
        let mail = input.mail;
        // Mail from Cuckoo's own domain is another agent or a loop: never work.
        let own = mail
            .from
            .as_deref()
            .is_some_and(|f| domain(f) == domain(&to));
        let verified = input.dmarc && !own && mail.from.as_deref().is_some_and(|f| f.len() <= 254);
        // An unverified claim can never match a binding, so it is held.
        let sender = match (&mail.from, verified) {
            (Some(from), true) => from.clone(),
            (from, _) => format!("unverified:{}", from.as_deref().unwrap_or("unknown"))
                .chars()
                .take(300)
                .collect(),
        };
        if verified {
            bind(&tx, &w, &id, &sender).await?;
        }
        let thread = if verified && !mail.automated {
            thread(&tx, &w, &id, &endpoint, &sender, &mail.parents).await?
        } else {
            None
        };
        let message = Message {
            event: input.event.clone(),
            sender: sender.clone(),
            recipient: to.clone(),
            conversation: sender.clone(),
            text: content(mail),
            attachments: vec![],
            automated: mail.automated || own,
            thread: None,
        };
        message.validate()?;
        let saved = self
            .receive_in_thread(&tx, &w, &endpoint, &message, thread.clone())
            .await?;
        if saved["duplicate"] != true
            && saved["status"] == "admitted"
            && let Some(chat_message) = saved["message"].as_str()
        {
            execute(&tx, "INSERT INTO relay_email_messages(workspace,endpoint,event,sender,message_id,subject,parents,chat_message,thread_root) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",
                vec![w.clone().into(), id.into(), input.event.clone().into(), sender.into(), mail.message_id.clone().into(),
                    mail.subject.clone().into(), json!(mail.parents).into(), chat_message.into(),
                    thread.unwrap_or_else(|| chat_message.to_owned()).into()]).await?;
        }
        tx.commit().await?;
        Ok(
            match (saved["duplicate"] == true, saved["status"].as_str()) {
                (true, _) => Outcome::Duplicate,
                (false, Some("admitted")) => Outcome::Admitted,
                (false, Some("held")) => Outcome::Held,
                _ => Outcome::Ignored,
            },
        )
    }
}

/// `nova.acme+anything@domain` reaches `nova.acme@domain`.
fn without_tag(address: &str) -> String {
    let address = address.to_lowercase();
    match address.split_once('@') {
        Some((local, domain)) => {
            format!("{}@{domain}", local.split('+').next().unwrap_or(local))
        }
        None => address,
    }
}
fn domain(address: &str) -> &str {
    address.rsplit_once('@').map_or("", |(_, d)| d)
}

/// What the agent and the room see: the subject, the new text, and a note when
/// attachments were left unread.
fn content(mail: &Mail) -> String {
    let mut text = String::new();
    if !mail.subject.is_empty() {
        text.push_str(&format!("**{}**\n\n", mail.subject.replace('*', "")));
    }
    text.push_str(if mail.text.trim().is_empty() {
        "(No text in this mail.)"
    } else {
        &mail.text
    });
    if mail.attachments > 0 {
        text.push_str(&format!(
            "\n\n[This mail had {} attachment{}. Cuckoo doesn't read email attachments yet.]",
            mail.attachments,
            if mail.attachments == 1 { "" } else { "s" }
        ));
    }
    text
}

/// A verified address belongs to the member who signs in with it. A sender
/// whose address now belongs to someone else is switched off, never moved.
async fn bind(tx: &DatabaseTransaction, w: &str, endpoint: &str, sender: &str) -> Result<()> {
    let people = rows(tx, "SELECT u.id FROM relay_users u JOIN relay_members m ON m.user_id=u.id AND m.workspace=$1 WHERE lower(u.email)=$2 LIMIT 2",
        vec![w.into(), sender.into()]).await?;
    let owner: Option<String> = match people.as_slice() {
        [one] => Some(one.try_get("", "id")?),
        _ => None,
    };
    let binding = query(tx, "SELECT user_id,enabled FROM relay_communication_bindings WHERE workspace=$1 AND endpoint=$2 AND sender=$3",
        vec![w.into(), endpoint.into(), sender.into()]).await?;
    match (binding, owner) {
        (Some(b), owner)
            if b.try_get::<bool>("", "enabled")?
                && owner.as_deref() != Some(b.try_get::<String>("", "user_id")?.as_str()) =>
        {
            execute(tx, "UPDATE relay_communication_bindings SET enabled=FALSE,revision=revision+1 WHERE workspace=$1 AND endpoint=$2 AND sender=$3",
                vec![w.into(), endpoint.into(), sender.into()]).await?;
            super::super::task_revocation::inaccessible(tx, w).await?;
            super::access::cancel_stale(tx, w).await?;
        }
        (None, Some(user)) => {
            execute(tx, "INSERT INTO relay_communication_bindings(workspace,endpoint,sender,user_id) VALUES($1,$2,$3,$4)",
                vec![w.into(), endpoint.into(), sender.into(), user.into()]).await?;
        }
        _ => {}
    }
    Ok(())
}

/// The room thread a mail continues: the newest of its references that is this
/// person's earlier mail to the agent, or a reply Cuckoo sent them (by SES's ID
/// or Cuckoo's own). `None` starts a new thread. A subject alone never joins one.
async fn thread(
    tx: &DatabaseTransaction,
    w: &str,
    endpoint: &str,
    e: &Value,
    sender: &str,
    parents: &[String],
) -> Result<Option<String>> {
    for parent in parents.iter().rev() {
        let local = parent.split_once('@').map_or(parent.as_str(), |(l, _)| l);
        let found = query(tx, "SELECT thread_root FROM relay_email_messages WHERE workspace=$1 AND endpoint=$2 AND sender=$3 AND message_id=$4
            UNION ALL
            SELECT m.thread_root FROM relay_communication_outbox o JOIN relay_email_messages m ON m.workspace=o.workspace AND m.endpoint=o.endpoint AND m.event=o.event
            WHERE o.workspace=$1 AND o.endpoint=$2 AND m.sender=$3 AND (o.provider_id=$5 OR o.attempt=$5)
            LIMIT 1", vec![w.into(), endpoint.into(), sender.into(), parent.clone().into(), local.into()]).await?;
        let Some(found) = found else { continue };
        let root: String = found.try_get("", "thread_root")?;
        // Still a top-level message of this person's direct room with the agent.
        let usable = query(tx, "SELECT 1 AS found FROM relay_chat_messages m JOIN relay_chats c ON c.workspace=m.workspace AND c.id=m.chat
            JOIN relay_communication_bindings b ON b.workspace=m.workspace AND b.endpoint=$3 AND b.sender=$4
            WHERE m.workspace=$1 AND m.id=$2 AND m.thread IS NULL AND c.kind='direct' AND c.direct_key IS NOT NULL
              AND EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id=b.user_id)
              AND EXISTS(SELECT 1 FROM relay_chat_agents a WHERE a.workspace=c.workspace AND a.chat=c.id AND a.agent=$5)",
            vec![w.into(), root.clone().into(), endpoint.into(), sender.into(), e["agent"].as_str().unwrap_or_default().into()]).await?;
        if usable.is_some() {
            return Ok(Some(root));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tags_are_ignored_and_content_names_the_subject_and_attachments() {
        assert_eq!(
            without_tag("Nova.Acme+leave@mail.cuckoo.onl"),
            "nova.acme@mail.cuckoo.onl"
        );
        let mail = Mail {
            message_id: None,
            from: Some("meera@acme.example".into()),
            subject: "Leave *balance*".into(),
            text: "How many days?".into(),
            parents: vec![],
            automated: false,
            attachments: 2,
        };
        let text = content(&mail);
        assert!(text.starts_with("**Leave balance**\n\nHow many days?"));
        assert!(text.contains("2 attachments"));
    }
}

//! Unconfirmed writes (schema 107). A write the provider didn't confirm holds every
//! later write on its account (`control_propose`, `claim`). It is settled when one
//! of the agent's managers or an admin says whether it happened, or when Cuckoo
//! finds at the provider that it did (`rechecks`). Settling releases the hold, and
//! the action's row trigger continues a task waiting for it (durable waits).
use super::*;
use crate::identity::control::outcomes::{AnswerConflict, Settle};
use serde_json::json;

#[path = "control_outcome_migration.rs"]
pub(in crate::identity::postgres) mod migration;
#[cfg(test)]
#[path = "control_outcome_tests.rs"]
mod tests;

/// The provider didn't confirm the action, or it has been executing for longer than
/// a dispatch takes: the 60-second rule Activity and the action views share.
const UNCONFIRMED: &str = "(a.state='uncertain' OR (a.state='executing' AND a.updated<floor(extract(epoch FROM clock_timestamp()))::bigint-60))";

/// What the agent reads in the action's result after it is settled.
const HAPPENED: &str = "A person checked the app: this happened.";
const NOT_SENT: &str = "A person checked the app: this was not sent, and nothing changed. Tell the person; send it again only if someone asks you to.";

fn unconfirmed(a: &Value) -> bool {
    a["state"] == "uncertain"
        || (a["state"] == "executing" && a["updated"].as_i64().unwrap_or(i64::MAX) < now() - 60)
}

/// `user` may say what happened (owner decision 2026-11-08): the agent's current
/// managers and admins who can see the request, but never the person who asked
/// when the action needed someone else's approval.
async fn may_settle(db: &impl ConnectionTrait, user: &User, w: &str, a: &Value) -> Result<()> {
    super::super::approval_delivery::viewer(db, user, w, a).await?;
    ensure!(
        !(a["needs_other"] == true && a["requester"] == user.id),
        "Someone other than the person who asked must say what happened"
    );
    let agent = a["agent"].as_str().context("Missing agent")?;
    if super::super::approval_authority::manages(db, w, agent, &user.id).await? {
        return Ok(());
    }
    ensure!(
        role(db, w, &user.id).await? == "Admin",
        "Only this agent's managers or an admin can say what happened"
    );
    Ok(())
}

/// `{by, by_name, at, happened, note}` once settled; `by` is null when Cuckoo found it.
async fn settled(db: &impl ConnectionTrait, w: &str, id: &str) -> Result<Value> {
    Ok(query(db, "SELECT jsonb_build_object('by',a.settled_by,'by_name',CASE WHEN a.settled_by IS NULL THEN 'Cuckoo' ELSE COALESCE(u.display_name,u.email,'Former member') END,
        'at',a.settled_at,'happened',a.state='completed','note',a.settled_note) AS data
        FROM relay_control_actions a LEFT JOIN relay_users u ON u.id=a.settled_by
        WHERE a.workspace=$1 AND a.id=$2 AND a.settled_at IS NOT NULL", vec![w.into(), id.into()])
        .await?
        .map(|row| row.try_get("", "data"))
        .transpose()?
        .unwrap_or(Value::Null))
}

/// The action as the outcome endpoint returns it.
async fn view(db: &impl ConnectionTrait, w: &str, id: &str) -> Result<Value> {
    let mut value = super::visible(db, w, id).await?;
    value["settled"] = settled(db, w, id).await?;
    Ok(value)
}

/// For the outcome dialog (the approval details): whether `user` may answer and
/// what Cuckoo already checked, while unconfirmed; the answer once settled.
pub(in crate::identity::postgres) async fn attach(
    db: &impl ConnectionTrait,
    user: &User,
    w: &str,
    a: &Value,
    value: &mut Value,
) -> Result<()> {
    let id = a["id"].as_str().context("Missing action")?;
    if value["state"] == "uncertain" {
        let checks = query(
            db,
            "SELECT checks FROM relay_control_outcome_checks WHERE workspace=$1 AND action=$2",
            vec![w.into(), id.into()],
        )
        .await?
        .map(|row| row.try_get::<Value>("", "checks"))
        .transpose()?
        .unwrap_or_else(|| json!([]));
        let can_settle = may_settle(db, user, w, a).await.is_ok();
        value["settle"] = json!({"can_settle": can_settle, "checks": checks});
    }
    let answer = settled(db, w, id).await?;
    if !answer.is_null() {
        value["settled"] = answer;
    }
    Ok(())
}

/// Tells open tabs through the existing change stream: the task's reply changed.
async fn announce(db: &impl ConnectionTrait, w: &str, job: &str) -> Result<()> {
    execute(
        db,
        "UPDATE relay_chat_messages SET content=content WHERE workspace=$1 AND id=$2",
        vec![w.into(), job.into()],
    )
    .await?;
    Ok(())
}

/// Moves an unconfirmed action to its settled state. The row update wakes the
/// waiting task and the approval card (durable waits, channel approvals).
/// False when it was no longer unconfirmed.
pub(super) async fn settle_row(
    db: &impl ConnectionTrait,
    w: &str,
    id: &str,
    by: Option<&str>,
    settled_as: &str,
    note: Option<&str>,
    result: Value,
) -> Result<bool> {
    let state = if settled_as == "not_happened" {
        "not_sent"
    } else {
        "completed"
    };
    let changed = execute(db, &format!("UPDATE relay_control_actions a SET state=$3,settled_by=$4,settled_at=$5,settled_as=$6,settled_note=$7,result=$8,updated=$5
        WHERE a.workspace=$1 AND a.id=$2 AND a.settled_at IS NULL AND {UNCONFIRMED}"),
        vec![w.into(), id.into(), state.into(), by.into(), now().into(), settled_as.into(), note.into(), result.into()]).await?;
    if changed != 1 {
        return Ok(false);
    }
    let job = query(
        db,
        "SELECT job FROM relay_control_actions WHERE workspace=$1 AND id=$2",
        vec![w.into(), id.into()],
    )
    .await?
    .context("Action unavailable")?
    .try_get::<String>("", "job")?;
    announce(db, w, &job).await?;
    Ok(true)
}

/// "said “Send an email” happened", shown in the Audit log after the person's name.
pub(super) fn summary(said: &str, title: &str) -> String {
    let title: String = title.chars().take(200).collect();
    format!("{said} “{title}”")
}

impl PgStore {
    /// A person says whether an unconfirmed action happened. The same answer again
    /// returns the saved one; the opposite answer is refused as a conflict.
    pub(crate) async fn control_settle(
        &self,
        user: &User,
        w: &str,
        id: &str,
        input: &Settle,
    ) -> Result<Value> {
        let note = input.note()?;
        let tx = self.workspace_tx(w).await?;
        let a = super::super::approval_delivery::action(&tx, w, id).await?;
        may_settle(&tx, user, w, &a).await?;
        if !a["settled_at"].is_null() {
            if (a["state"] == "completed") != input.happened {
                return Err(AnswerConflict.into());
            }
            let value = view(&tx, w, id).await?;
            tx.commit().await?;
            return Ok(value);
        }
        ensure!(
            unconfirmed(&a),
            "Only an action whose outcome wasn't confirmed can be answered"
        );
        let (settled_as, message, said) = if input.happened {
            ("happened", HAPPENED, "said this happened:")
        } else {
            ("not_happened", NOT_SENT, "said this was not sent:")
        };
        let result = json!({"message": message, "note": note});
        ensure!(
            settle_row(
                &tx,
                w,
                id,
                Some(&user.id),
                settled_as,
                note.as_deref(),
                result
            )
            .await?,
            "This action changed; reload it"
        );
        let title = a["title"].as_str().unwrap_or_default();
        audit_with_summary(
            &tx,
            &user.id,
            Some(w),
            "control_action_settled",
            id,
            &summary(said, title),
        )
        .await?;
        let value = view(&tx, w, id).await?;
        tx.commit().await?;
        Ok(value)
    }
}

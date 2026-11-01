use super::*;
use crate::identity::{
    chat::model::Job,
    communications::model::{Outcome, Receipt},
};
use serde_json::json;

pub(in crate::identity::postgres) async fn complete(
    tx: &DatabaseTransaction,
    job: &Job,
    content: &str,
) -> Result<()> {
    if !super::access::exists(tx).await? {
        return Ok(());
    }
    let Some(inbox)=json_rows(tx,"SELECT to_jsonb(i) AS data FROM relay_communication_inbox i WHERE workspace=$1 AND reply=$2",vec![job.workspace.clone().into(),job.reply.clone().into()]).await?.into_iter().next() else {return Ok(());};
    let chat = inbox["transport"] == "chat";
    let original = &inbox["envelope"];
    let envelope = json!({"event":job.reply,"sender":if chat {json!(job.agent)} else {original["recipient"].clone()},
        "recipient":if chat {json!(job.chat)} else {original["sender"].clone()},
        "conversation":original["conversation"],"thread":original["thread"],"text":content,"attachments":[],"automated":true});
    execute(tx,"INSERT INTO relay_communication_outbox(workspace,id,endpoint,event,transport,envelope,job,source_claim,state) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING",vec![job.workspace.clone().into(),job.reply.clone().into(),inbox["endpoint"].as_str().unwrap().into(),inbox["event"].as_str().unwrap().into(),inbox["transport"].as_str().unwrap().into(),envelope.into(),serde_json::to_value(job)?.into(),job.claim.clone().context("Missing completion claim")?.into(),if chat {"published"} else {"queued"}.into()]).await?;
    Ok(())
}

impl PgStore {
    pub(crate) async fn simulate_dispatch(
        &self,
        actor: &User,
        w: &str,
        id: &str,
        outcome: Outcome,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        super::access::owner(&tx, actor, w).await?;
        let row = load(&tx, w, id).await?;
        ensure!(
            row["transport"] != "chat",
            "Chat publication is committed with its reply"
        );
        let e = super::endpoints::endpoint(&tx, w, row["endpoint"].as_str().unwrap())
            .await?
            .context("Endpoint unavailable")?;
        ensure!(e["simulated"] == true, "Live dispatch is not implemented");
        if row["state"] == "sending" {
            // A crashed process could have sent it. Expiry never makes it retryable.
            execute(&tx,"UPDATE relay_communication_outbox SET state='uncertain' WHERE workspace=$1 AND id=$2 AND state='sending' AND lease_until<=floor(extract(epoch FROM clock_timestamp()))::bigint",vec![w.into(),id.into()]).await?;
            let result = metadata(&load(&tx, w, id).await?);
            tx.commit().await?;
            return Ok(result);
        }
        if ![json!("queued"), json!("failed")].contains(&row["state"]) {
            return Ok(metadata(&row));
        }
        ensure!(
            row["attempts"].as_i64().unwrap() < 3,
            "Simulation send attempts exhausted"
        );
        let current = deliverable(&tx, w, &row).await?;
        if !current {
            execute(&tx,"UPDATE relay_communication_outbox SET state='cancelled' WHERE workspace=$1 AND id=$2",vec![w.into(),id.into()]).await?;
            let result = metadata(&load(&tx, w, id).await?);
            tx.commit().await?;
            return Ok(result);
        }
        let attempt = uuid::Uuid::new_v4().to_string();
        execute(&tx,"UPDATE relay_communication_outbox SET state='sending',attempt=$3,provider_id=NULL,attempts=attempts+1,lease_until=floor(extract(epoch FROM clock_timestamp()))::bigint+30 WHERE workspace=$1 AND id=$2",vec![w.into(),id.into(),attempt.clone().into()]).await?;
        tx.commit().await?;
        // No network: the fixture models the provider's result after durable intent.
        // A future adapter receives this immutable envelope and attempt key only.
        let tx = self.workspace_tx(w).await?;
        let (state, provider) = match outcome {
            Outcome::Accepted => ("accepted", Some(provider_id(id, &attempt))),
            Outcome::Delivered => ("delivered", Some(provider_id(id, &attempt))),
            Outcome::Rejected => ("failed", None),
            Outcome::Timeout => ("uncertain", None),
            Outcome::Crash => ("sending", None),
        };
        execute(&tx,"UPDATE relay_communication_outbox SET state=$4,provider_id=$5 WHERE workspace=$1 AND id=$2 AND attempt=$3 AND state='sending'",vec![w.into(),id.into(),attempt.into(),state.into(),provider.into()]).await?;
        let result = metadata(&load(&tx, w, id).await?);
        tx.commit().await?;
        Ok(result)
    }
    pub(crate) async fn simulate_receipt(
        &self,
        actor: &User,
        w: &str,
        id: &str,
        input: &Receipt,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        super::access::owner(&tx, actor, w).await?;
        let row = load(&tx, w, id).await?;
        ensure!(
            row["transport"] != "chat"
                && row["attempt"] == input.attempt
                && input.provider_id == provider_id(id, &input.attempt),
            "Receipt does not match the delivery attempt"
        );
        ensure!(
            [
                json!("sending"),
                json!("uncertain"),
                json!("accepted"),
                json!("delivered")
            ]
            .contains(&row["state"]),
            "This attempt cannot accept a delivery receipt"
        );
        // Late acceptance never downgrades delivery, including duplicate callbacks.
        execute(&tx,"UPDATE relay_communication_outbox SET state=CASE WHEN state='delivered' OR $4 THEN 'delivered' ELSE 'accepted' END,provider_id=$3 WHERE workspace=$1 AND id=$2",vec![w.into(),id.into(),input.provider_id.clone().into(),input.delivered.into()]).await?;
        let result = metadata(&load(&tx, w, id).await?);
        tx.commit().await?;
        Ok(result)
    }
}
/// Whether a saved reply may still go out: the request's sources, context,
/// conversation and agent access still hold, and the reply text is unchanged.
pub(super) async fn deliverable(tx: &DatabaseTransaction, w: &str, row: &Value) -> Result<bool> {
    let id = row["id"]
        .as_str()
        .context("Communication output unavailable")?;
    let mut job: Job = serde_json::from_value(row["job"].clone())?;
    job.claim = Some(
        row["source_claim"]
            .as_str()
            .context("Missing source claim")?
            .into(),
    );
    let user = User {
        id: job.requester.clone(),
        email: String::new(),
        display_name: None,
    };
    Ok(super::super::source_access::allowed(tx,&job).await?
        && super::super::context_attempts::allowed(tx,&job).await?
        && super::super::chat::access(tx,&user,w,&job.chat).await.is_ok()
        && super::super::chat::agent_access(tx,&user.id,w,&job.agent).await.unwrap_or(false)
        && query(tx,"SELECT id FROM relay_chat_messages WHERE workspace=$1 AND id=$2 AND state='complete' AND content=$3",vec![w.into(),id.into(),row["envelope"]["text"].as_str().context("Missing output")?.into()]).await?.is_some())
}
pub(super) async fn load(tx: &DatabaseTransaction, w: &str, id: &str) -> Result<Value> {
    json_rows(
        tx,
        "SELECT to_jsonb(o) AS data FROM relay_communication_outbox o WHERE workspace=$1 AND id=$2",
        vec![w.into(), id.into()],
    )
    .await?
    .into_iter()
    .next()
    .context("Communication output unavailable")
}
pub(super) fn metadata(row: &Value) -> Value {
    json!({"id":row["id"],"state":row["state"],"attempt":row["attempt"],"provider_id":row["provider_id"],"attempts":row["attempts"],"reason":row["reason"]})
}
fn provider_id(id: &str, attempt: &str) -> String {
    format!("sim:{id}:{attempt}")
}

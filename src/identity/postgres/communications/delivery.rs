//! The gateway's side of live transports: where each server was read up to,
//! incoming messages, replies to send and what became of each send. Network
//! calls happen between these steps, never inside a transaction.
use super::*;
use crate::identity::communications::{
    adapters::{Outgoing, Route, Sent, Source},
    model::Message,
};

const LEASE_SECONDS: i64 = 30;
/// A transport server that could not be reached is tried again after this.
const RETRY_SECONDS: i64 = 30;
/// An unclear send is asked about again after this.
const RECHECK_SECONDS: i64 = 60;
const NOW: &str = "floor(extract(epoch FROM clock_timestamp()))::bigint";

/// A reply claimed for sending (or for asking what became of it).
pub(crate) struct Delivery {
    pub workspace: String,
    pub id: String,
    pub attempt: String,
    pub transport: String,
    pub route: Route,
    pub outgoing: Outgoing,
}

impl PgStore {
    /// Servers with an enabled live endpoint, and where each was read up to.
    pub(crate) async fn transport_sources(&self) -> Result<Vec<(Source, Option<String>)>> {
        let found = rows(&self.db, "SELECT DISTINCT e.workspace,e.transport,e.server,c.cursor FROM relay_communication_endpoints e
            LEFT JOIN relay_communication_cursors c ON c.workspace=e.workspace AND c.transport=e.transport AND c.server=e.server
            WHERE NOT e.simulated AND e.enabled AND e.server IS NOT NULL ORDER BY 1,2,3 LIMIT 500", vec![]).await?;
        found
            .iter()
            .map(|r| {
                Ok((
                    Source {
                        workspace: r.try_get("", "workspace")?,
                        transport: r.try_get("", "transport")?,
                        server: r.try_get("", "server")?,
                    },
                    r.try_get("", "cursor")?,
                ))
            })
            .collect()
    }

    /// Moves a server's cursor, unless another controller already moved it.
    pub(crate) async fn save_transport_cursor(
        &self,
        source: &Source,
        from: Option<&str>,
        next: &str,
    ) -> Result<()> {
        let values = vec![
            source.workspace.clone().into(),
            source.transport.clone().into(),
            source.server.clone().into(),
            next.into(),
            from.into(),
        ];
        if from.is_none() {
            execute(&self.db, &format!("INSERT INTO relay_communication_cursors(workspace,transport,server,cursor,updated) VALUES($1,$2,$3,$4,{NOW}) ON CONFLICT DO NOTHING"), values[..4].to_vec()).await?;
        } else {
            execute(&self.db, &format!("UPDATE relay_communication_cursors SET cursor=$4,updated={NOW} WHERE workspace=$1 AND transport=$2 AND server=$3 AND cursor=$5"), values).await?;
        }
        Ok(())
    }

    /// One message a transport adapter read from its platform. The endpoint it
    /// was sent to decides the workspace and agent; the message decides nothing.
    pub(crate) async fn transport_incoming(
        &self,
        source: &Source,
        input: &Message,
    ) -> Result<Value> {
        input.validate()?;
        let w = &source.workspace;
        let tx = self.workspace_tx(w).await?;
        let endpoint = json_rows(&tx, "SELECT to_jsonb(e) AS data FROM relay_communication_endpoints e WHERE workspace=$1 AND transport=$2 AND server=$3 AND address=$4 AND NOT simulated",
            vec![w.into(), source.transport.clone().into(), source.server.clone().into(), input.recipient.clone().into()]).await?.into_iter().next().context("No endpoint for this address")?;
        let result = self.receive(&tx, w, &endpoint, input).await?;
        tx.commit().await?;
        Ok(result)
    }

    /// A platform's delivery receipt for one attempt. Never downgrades delivery.
    pub(crate) async fn transport_receipt(
        &self,
        source: &Source,
        key: &str,
        delivered: bool,
    ) -> Result<()> {
        let tx = self.workspace_tx(&source.workspace).await?;
        execute(&tx, "UPDATE relay_communication_outbox o SET state=CASE WHEN $5 THEN 'delivered' ELSE 'failed' END,
            reason=CASE WHEN $5 THEN o.reason ELSE 'The platform reported the message failed' END
            WHERE o.workspace=$1 AND o.attempt=$4 AND o.state IN ('sending','uncertain','accepted')
            AND EXISTS(SELECT 1 FROM relay_communication_endpoints e WHERE e.workspace=o.workspace AND e.id=o.endpoint AND e.transport=$2 AND e.server=$3 AND NOT e.simulated)",
            vec![source.workspace.clone().into(), source.transport.clone().into(), source.server.clone().into(), key.into(), delivered.into()]).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Claims up to `limit` replies ready to go out on live transports. A reply
    /// whose request lost its access is cancelled instead of sent.
    pub(crate) async fn claim_deliveries(&self, limit: i64) -> Result<Vec<Delivery>> {
        let candidates = rows(&self.db, &format!("SELECT o.workspace,o.id FROM relay_communication_outbox o
            JOIN relay_communication_endpoints e ON e.workspace=o.workspace AND e.id=o.endpoint
            WHERE o.state='queued' AND o.lease_until<={NOW} AND NOT e.simulated ORDER BY o.sequence LIMIT $1"), vec![limit.into()]).await?;
        let mut claimed = vec![];
        for candidate in candidates {
            let (w, id): (String, String) = (
                candidate.try_get("", "workspace")?,
                candidate.try_get("", "id")?,
            );
            let tx = self.workspace_tx(&w).await?;
            let Some(row) = json_rows(&tx, &format!("SELECT to_jsonb(o) AS data FROM relay_communication_outbox o WHERE workspace=$1 AND id=$2 AND state='queued' AND lease_until<={NOW} FOR UPDATE SKIP LOCKED"),
                vec![w.clone().into(), id.clone().into()]).await?.into_iter().next() else { continue };
            if !super::outbox::deliverable(&tx, &w, &row).await? {
                execute(&tx, "UPDATE relay_communication_outbox SET state='cancelled',reason='The request lost its access before sending' WHERE workspace=$1 AND id=$2", vec![w.clone().into(), id.clone().into()]).await?;
                tx.commit().await?;
                continue;
            }
            let attempt = uuid::Uuid::new_v4().to_string();
            execute(&tx, &format!("UPDATE relay_communication_outbox SET state='sending',attempt=$3,provider_id=NULL,reason=NULL,attempts=attempts+1,lease_until={NOW}+{LEASE_SECONDS} WHERE workspace=$1 AND id=$2"),
                vec![w.clone().into(), id.clone().into(), attempt.clone().into()]).await?;
            let delivery = delivery(&tx, &w, &row, attempt).await?;
            tx.commit().await?;
            claimed.push(delivery);
        }
        Ok(claimed)
    }

    /// Records what the platform said about a claimed send.
    pub(crate) async fn finish_delivery(&self, d: &Delivery, sent: &Sent) -> Result<()> {
        let (sql, values): (String, Vec<SqlValue>) = match sent {
            Sent::Accepted(p) | Sent::Delivered(p) => (
                "SET state=$4,provider_id=$5".into(),
                vec![
                    if matches!(sent, Sent::Delivered(_)) {
                        "delivered"
                    } else {
                        "accepted"
                    }
                    .into(),
                    p.clone().into(),
                ],
            ),
            Sent::Refused(r) => (
                "SET state='failed',reason=$4".into(),
                vec![r.clone().into()],
            ),
            // Never reached the platform: this attempt doesn't count.
            Sent::Unreached(r) => (
                format!(
                    "SET state='queued',attempt=NULL,attempts=attempts-1,reason=$4,lease_until={NOW}+{RETRY_SECONDS}"
                ),
                vec![r.clone().into()],
            ),
            Sent::Uncertain(r) => (
                format!("SET state='uncertain',reason=$4,lease_until={NOW}+{RECHECK_SECONDS}"),
                vec![r.clone().into()],
            ),
        };
        let mut all: Vec<SqlValue> = vec![
            d.workspace.clone().into(),
            d.id.clone().into(),
            d.attempt.clone().into(),
        ];
        all.extend(values);
        let tx = self.workspace_tx(&d.workspace).await?;
        execute(&tx, &format!("UPDATE relay_communication_outbox {sql} WHERE workspace=$1 AND id=$2 AND attempt=$3 AND state='sending'"), all).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Unclear sends due to be asked about again. A send whose controller
    /// stopped mid-way becomes unclear first: it may have reached the platform.
    pub(crate) async fn uncertain_deliveries(&self, limit: i64) -> Result<Vec<Delivery>> {
        execute(&self.db, &format!("UPDATE relay_communication_outbox o SET state='uncertain',reason='Cuckoo stopped while sending' FROM relay_communication_endpoints e
            WHERE e.workspace=o.workspace AND e.id=o.endpoint AND NOT e.simulated AND o.state='sending' AND o.lease_until<={NOW}"), vec![]).await?;
        let due = rows(&self.db, &format!("SELECT o.workspace,o.id FROM relay_communication_outbox o
            JOIN relay_communication_endpoints e ON e.workspace=o.workspace AND e.id=o.endpoint
            WHERE o.state='uncertain' AND o.attempt IS NOT NULL AND o.lease_until<={NOW} AND NOT e.simulated ORDER BY o.sequence LIMIT $1"), vec![limit.into()]).await?;
        let mut found = vec![];
        for candidate in due {
            let (w, id): (String, String) = (
                candidate.try_get("", "workspace")?,
                candidate.try_get("", "id")?,
            );
            let tx = self.workspace_tx(&w).await?;
            let Some(row) = json_rows(&tx, &format!("SELECT to_jsonb(o) AS data FROM relay_communication_outbox o WHERE workspace=$1 AND id=$2 AND state='uncertain' AND lease_until<={NOW} FOR UPDATE SKIP LOCKED"),
                vec![w.clone().into(), id.clone().into()]).await?.into_iter().next() else { continue };
            execute(&tx, &format!("UPDATE relay_communication_outbox SET lease_until={NOW}+{RECHECK_SECONDS} WHERE workspace=$1 AND id=$2"), vec![w.clone().into(), id.into()]).await?;
            let attempt = row["attempt"]
                .as_str()
                .context("Missing attempt")?
                .to_owned();
            let delivery = delivery(&tx, &w, &row, attempt).await?;
            tx.commit().await?;
            found.push(delivery);
        }
        Ok(found)
    }

    /// Records the platform's later answer about an unclear send.
    pub(crate) async fn settle_delivery(&self, d: &Delivery, sent: &Sent) -> Result<()> {
        let (state, provider, reason) = match sent {
            Sent::Accepted(p) => ("accepted", p.clone(), None),
            Sent::Delivered(p) => ("delivered", p.clone(), None),
            Sent::Refused(r) => ("failed", None, Some(r.clone())),
            Sent::Unreached(_) | Sent::Uncertain(_) => return Ok(()),
        };
        let tx = self.workspace_tx(&d.workspace).await?;
        execute(&tx, "UPDATE relay_communication_outbox SET state=$4,provider_id=COALESCE($5,provider_id),reason=COALESCE($6,reason) WHERE workspace=$1 AND id=$2 AND attempt=$3 AND state='uncertain'",
            vec![d.workspace.clone().into(), d.id.clone().into(), d.attempt.clone().into(), state.into(), provider.into(), reason.into()]).await?;
        tx.commit().await?;
        Ok(())
    }
}

/// The send exactly as saved: the room's recipient, never one the agent chose.
async fn delivery(
    tx: &DatabaseTransaction,
    w: &str,
    row: &Value,
    attempt: String,
) -> Result<Delivery> {
    let e = super::endpoints::endpoint(
        tx,
        w,
        row["endpoint"].as_str().context("Endpoint unavailable")?,
    )
    .await?
    .context("Endpoint unavailable")?;
    let envelope = &row["envelope"];
    let field = |name: &str| {
        envelope[name]
            .as_str()
            .map(str::to_owned)
            .context("Incomplete reply envelope")
    };
    Ok(Delivery {
        workspace: w.into(),
        id: row["id"].as_str().context("Missing reply")?.into(),
        transport: e["transport"].as_str().context("Missing transport")?.into(),
        route: Route {
            workspace: w.into(),
            server: e["server"]
                .as_str()
                .context("Missing transport server")?
                .into(),
            address: e["address"].as_str().context("Missing address")?.into(),
        },
        outgoing: Outgoing {
            key: attempt.clone(),
            to: field("recipient")?,
            conversation: field("conversation")?,
            thread: envelope["thread"].as_str().map(str::to_owned),
            text: field("text")?,
        },
        attempt,
    })
}

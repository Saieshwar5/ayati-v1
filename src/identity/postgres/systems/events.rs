//! Events from connected systems (schema 117, C2 and C5 of the connected
//! systems plan, D249): the events key, intake, Recent events and the event
//! types the automation form offers. Each type an automation listens to is a
//! `system` source in relay_connector_subscriptions, so receipts, deliveries,
//! dispatch, work rooms and Activity are the usual ones.
use crate::identity::events::system_events::{self, Envelope};
use crate::identity::postgres::*;
use serde_json::json;

/// Events kept for Recent events and duplicate checks.
const KEEP_MS: i64 = 30 * 86_400_000;

/// The key was wrong, or the system's events are off or it was removed (401).
#[derive(Debug)]
pub(crate) struct Refused;
impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Wrong key, or this system's events are off")
    }
}
impl std::error::Error for Refused {}

/// The same event id was sent before with different content (409).
#[derive(Debug)]
pub(crate) struct Conflict;
impl std::fmt::Display for Conflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("This event id was already used for a different event")
    }
}
impl std::error::Error for Conflict {}

/// Compares digests in constant time.
fn same(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

pub(super) async fn ready(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regclass('relay_system_events') IS NOT NULL AS ready",
        vec![],
    )
    .await?
    .context("Missing schema check")?
    .try_get("", "ready")?)
}

impl PgStore {
    /// Turns a system's events on with a new key (shown once, kept as its
    /// digest), or off. Turning them on again replaces the key at once.
    pub(crate) async fn system_events_key(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        id: &str,
        on: bool,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        super::connect::may_change(&tx, user, w, agent).await?;
        super::connect::saved(&tx, w, agent, id).await?;
        ensure!(
            ready(&tx).await?,
            "Run workspace-db migrate to receive events"
        );
        let key = on.then(|| format!("cke_{}", crate::identity::provider::random_token()));
        execute(&tx, "UPDATE relay_connected_systems SET events_on=$3,events_key=CASE WHEN $3 THEN $4 ELSE events_key END,revision=revision+1 WHERE workspace=$1 AND connection=$2",
            vec![w.into(), id.into(), on.into(), key.as_deref().map(crate::identity::provider::digest).into()]).await?;
        audit(
            &tx,
            &user.id,
            Some(w),
            if on {
                "connected_system_events_key"
            } else {
                "connected_system_events_off"
            },
            &format!("{agent}/{id}"),
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"events_on":on,"key":key}))
    }

    /// The workspace of a system whose events are on and whose key matches.
    pub(super) async fn system_for_key(&self, connection: &str, key: &str) -> Result<String> {
        crate::identity::worker_control::model::uuid(connection).map_err(|_| Refused)?;
        let row = query(&self.db, "SELECT s.workspace,s.events_key FROM relay_connected_systems s JOIN relay_control_connections c ON c.workspace=s.workspace AND c.id=s.connection
            WHERE s.connection=$1 AND s.events_on AND NOT s.removed AND c.enabled", vec![connection.into()]).await?;
        let Some(row) = row else {
            return Err(Refused.into());
        };
        let saved: Option<String> = row.try_get("", "events_key")?;
        if !saved.is_some_and(|d| same(&d, &crate::identity::provider::digest(key))) {
            return Err(Refused.into());
        }
        Ok(row.try_get("", "workspace")?)
    }

    /// `POST /hooks/system/{connection}`: one event from a system, with its key.
    /// The receipt is saved before the answer. A repeat with the same content is
    /// a duplicate; the same id with other content is refused.
    pub(crate) async fn system_event(
        &self,
        connection: &str,
        key: &str,
        body: &Value,
        receipt_limit: i64,
    ) -> Result<Value> {
        let w = self.system_for_key(connection, key).await?;
        let event = system_events::envelope(body)?;
        self.system_event_in(&w, connection, &event, body, false, receipt_limit)
            .await
    }

    /// "Send test event" from the connection tile: the same path, marked as a test.
    pub(crate) async fn system_test_event(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        id: &str,
        sample: &Value,
        receipt_limit: i64,
    ) -> Result<Value> {
        let (kind, data) = (sample["type"].as_str().unwrap_or_default(), &sample["data"]);
        {
            let tx = self.read_tx(w).await?;
            super::connect::may_change(&tx, user, w, agent).await?;
            super::connect::saved(&tx, w, agent, id).await?;
            tx.commit().await?;
        }
        let body =
            json!({"id":format!("test-{}", uuid::Uuid::new_v4().simple()),"type":kind,"data":data});
        let event = system_events::envelope(&body)?;
        self.system_event_in(w, id, &event, &body, true, receipt_limit)
            .await
    }

    async fn system_event_in(
        &self,
        w: &str,
        connection: &str,
        event: &Envelope,
        body: &Value,
        test: bool,
        receipt_limit: i64,
    ) -> Result<Value> {
        ensure!(self.pooled, "Event intake requires pooled execution");
        let hash = crate::identity::events::model::fingerprint(body)?;
        if let Some(row) = query(
            &self.db,
            "SELECT hash FROM relay_system_events WHERE workspace=$1 AND connection=$2 AND id=$3",
            vec![w.into(), connection.into(), event.id.clone().into()],
        )
        .await?
        {
            if row.try_get::<String>("", "hash")? != hash {
                return Err(Conflict.into());
            }
            return Ok(json!({"status":"duplicate"}));
        }
        // Every automation listening to this type has its own source.
        let sources = rows(&self.db, "SELECT id FROM relay_connector_subscriptions WHERE workspace=$1 AND connection=$2 AND provider='system' AND event=$3 AND state='active'",
            vec![w.into(), connection.into(), event.kind.clone().into()]).await?;
        let mut matched = 0;
        for row in sources {
            let id: String = row.try_get("", "id")?;
            let Some(source) = super::super::event_subscriptions::source(&self.db, w, &id).await?
            else {
                continue;
            };
            let accepted = self
                .accept_event(&source, &event.id, body, receipt_limit)
                .await?;
            if accepted["duplicate"] != true {
                matched += 1;
            }
        }
        let tx = self.workspace_tx(w).await?;
        execute(&tx, "INSERT INTO relay_system_events(workspace,connection,id,type,subject,hash,fields,matched,test) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING",
            vec![w.into(), connection.into(), event.id.clone().into(), event.kind.clone().into(), event.subject.clone().into(), hash.into(),
                json!(system_events::field_names(&event.data)).into(), matched.into(), test.into()]).await?;
        execute(&tx, "DELETE FROM relay_system_events WHERE workspace=$1 AND connection=$2 AND received_ms<(extract(epoch FROM clock_timestamp())*1000)::bigint-$3",
            vec![w.into(), connection.into(), KEEP_MS.into()]).await?;
        tx.commit().await?;
        Ok(json!({"status":"accepted","matched":matched,"id":event.id}))
    }

    /// Recent events (latest 100) and the event types received so far, with
    /// their field names, for the tile and the automation form. Never values.
    pub(crate) async fn system_events(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        id: &str,
    ) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        super::super::agent_profiles::managed_access(&tx, &user.id, w, agent, false).await?;
        let system = super::connect::saved(&tx, w, agent, id).await?;
        if !ready(&tx).await? {
            tx.commit().await?;
            return Ok(json!({"events_on":false,"recent":[],"types":[]}));
        }
        let on: bool = query(
            &tx,
            "SELECT events_on FROM relay_connected_systems WHERE workspace=$1 AND connection=$2",
            vec![w.into(), id.into()],
        )
        .await?
        .context("System")?
        .try_get("", "events_on")?;
        let recent = json_rows(&tx, "SELECT jsonb_build_object('id',id,'type',type,'subject',subject,'fields',fields,'matched',matched,'test',test,'received_ms',received_ms) AS data
            FROM relay_system_events WHERE workspace=$1 AND connection=$2 ORDER BY received_ms DESC,id LIMIT 100", vec![w.into(), id.into()]).await?;
        let types = json_rows(&tx, "SELECT jsonb_build_object('type',t.type,'count',t.count,'last_ms',t.last_ms,'fields',
                (SELECT e.fields FROM relay_system_events e WHERE e.workspace=$1 AND e.connection=$2 AND e.type=t.type ORDER BY e.received_ms DESC LIMIT 1)) AS data
            FROM (SELECT type,count(*) AS count,max(received_ms) AS last_ms FROM relay_system_events WHERE workspace=$1 AND connection=$2 GROUP BY type) t
            ORDER BY t.last_ms DESC LIMIT 50", vec![w.into(), id.into()]).await?;
        tx.commit().await?;
        Ok(json!({"events_on":on,"name":system["name"],"recent":recent,"types":types}))
    }
}

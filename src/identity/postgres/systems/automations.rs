//! Automations that start when a connected system sends an event (C2). An
//! automation names one event type and the `data` fields the agent sees; the
//! controller keeps one `system` source per system, type and field choice, and
//! the trigger is an ordinary event trigger on it, so it runs in a work room,
//! has a Memory setting and shows in Activity like any other.
use crate::identity::events::{
    catalog::Source,
    model::{Trigger, fingerprint},
    system_events,
};
use crate::identity::postgres::*;
use serde::{Deserialize, Serialize};
use serde_json::json;

/// "When <system> sends <type>, do …".
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FromSystem {
    pub id: String,
    pub system: String,
    pub event: String,
    /// The top-level `data` fields the agent sees (1 to 8), whole.
    pub fields: Vec<String>,
    pub name: String,
    pub instructions: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<String>,
    #[serde(default = "empty")]
    pub filter: Value,
    /// Named, typed values the agent must report (C4), at most 8.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub results: Vec<super::results::ResultField>,
}
fn empty() -> Value {
    json!({})
}
impl FromSystem {
    fn validate(&self) -> Result<()> {
        crate::identity::automations::model::uuid(&self.system)?;
        ensure!(
            system_events::valid_type(&self.event),
            "Choose an event type: lowercase letters, digits, dots or underscores"
        );
        ensure!(
            !self.fields.is_empty() && self.fields.len() <= 8,
            "Choose between 1 and 8 fields the agent sees"
        );
        let mut seen = std::collections::HashSet::new();
        for field in &self.fields {
            crate::identity::identifier(field)
                .map_err(|_| anyhow::anyhow!("Field names use letters, digits, - and _"))?;
            ensure!(seen.insert(field), "A field is chosen twice");
        }
        super::results::check_fields(&self.results)?;
        ensure!(
            self.memory
                .as_deref()
                .is_none_or(|m| ["fresh", "remember", "subject"].contains(&m)),
            "Choose what the automation remembers: fresh, remember or per subject"
        );
        // "Per subject" exists only for systems; the rest is any trigger's check.
        let mut checked = self.trigger(
            "00000000-0000-4000-8000-000000000000",
            &"a".repeat(43),
            1,
            1,
        );
        checked.memory = None;
        checked.validate()
    }
    fn trigger(&self, source: &str, binding: &str, connection: i64, read: i64) -> Trigger {
        Trigger {
            id: self.id.clone(),
            source: source.to_owned(),
            binding: binding.to_owned(),
            chat: self.chat.clone(),
            memory: self.memory.clone(),
            name: self.name.clone(),
            instructions: self.instructions.clone(),
            connection_revision: connection,
            read_revision: read,
            delegation_revision: None,
            filter: self.filter.clone(),
        }
    }
}

/// The system's reading tool the event stands in for: the first one the agent
/// may use without asking. An event carries what such a read could fetch.
async fn read_tool(
    tx: &DatabaseTransaction,
    w: &str,
    agent: &str,
    connection: &str,
) -> Result<String> {
    let row = query(tx, "SELECT r.operation FROM relay_control_effective_rules r JOIN relay_control_connections c ON c.workspace=r.workspace AND c.id=r.connection
        CROSS JOIN LATERAL jsonb_array_elements(c.definition->'operations') WITH ORDINALITY AS o(op,ord)
        WHERE r.workspace=$1 AND r.agent=$2 AND r.connection=$3 AND r.mode='allow' AND o.op->>'id'=r.operation AND o.op->>'effect'='read'
        ORDER BY o.ord LIMIT 1", vec![w.into(), agent.into(), connection.into()]).await?;
    row.map(|r| r.try_get::<String>("", "operation"))
        .transpose()?
        .context("Set one of this system's reading tools to Allow first: an event can only start work its agent may read")
}

/// The active source for this system, type and fields, made when needed. A
/// source made before the system was reviewed again is retired, so a new
/// automation always matches the tools as they are now.
async fn source(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    input: &FromSystem,
    read: &str,
) -> Result<Source> {
    let connection = query(tx, "SELECT c.fingerprint,s.name FROM relay_control_connections c JOIN relay_connected_systems s ON s.workspace=c.workspace AND s.connection=c.id WHERE c.workspace=$1 AND c.id=$2",
        vec![w.into(), input.system.clone().into()]).await?.context("System unavailable")?;
    let fingerprint: String = connection.try_get("", "fingerprint")?;
    let name: String = connection.try_get("", "name")?;
    let mut fields = input.fields.clone();
    fields.sort();
    let config_hash = crate::identity::provider::digest(&json!(fields).to_string());
    let condition = "s.workspace=$1 AND s.connection=$2 AND s.provider='system' AND s.event=$3 AND s.config_hash=$4 AND s.state='active'";
    if let Some(row) = super::super::event_subscriptions::row(
        tx,
        condition,
        vec![
            w.into(),
            input.system.clone().into(),
            input.event.clone().into(),
            config_hash.clone().into(),
        ],
    )
    .await?
    {
        if row["connection_fingerprint"] == fingerprint.as_str() && row["read_operation"] == read {
            return super::super::event_subscriptions::source_from(&row);
        }
        execute(tx, "UPDATE relay_connector_subscriptions SET state='removed',released=true,detail='The system was reviewed again.',changed_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE workspace=$1 AND id=$2",
            vec![w.into(), row["id"].as_str().unwrap_or_default().into()]).await?;
    }
    let id = uuid::Uuid::new_v4().to_string();
    let map: serde_json::Map<String, Value> =
        fields.iter().map(|f| (f.clone(), json!(f))).collect();
    execute(tx, "INSERT INTO relay_connector_subscriptions(workspace,id,connection,event,provider,provider_id,trigger,config,config_hash,fields,binding,connection_fingerprint,read_operation,name,created_by)
        VALUES($1,$2,$3,$4,'system',$2,$4,'{}'::jsonb,$5,$6,'{}'::jsonb,$7,$8,$9,$10)",
        vec![w.into(), id.clone().into(), input.system.clone().into(), input.event.clone().into(), config_hash.into(),
            Value::Object(map).into(), fingerprint.into(), read.into(), format!("{name}: {}", input.event).into(), user.id.clone().into()]).await?;
    super::super::event_subscriptions::source(tx, w, &id)
        .await?
        .context("The event source was not saved")
}

impl PgStore {
    /// Saves "when <system> sends <type>" for `agent`. The same request again
    /// returns the saved automation; the same id otherwise is refused.
    pub(crate) async fn create_system_trigger(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        input: &FromSystem,
    ) -> Result<Value> {
        input.validate()?;
        ensure!(
            self.pooled,
            "Event work requires pooled background execution"
        );
        let hash = fingerprint(input)?;
        let tx = self.workspace_tx(w).await?;
        if let Some(row) = query(&tx, "SELECT principal,agent,request_hash FROM relay_event_triggers WHERE workspace=$1 AND id=$2",
            vec![w.into(), input.id.clone().into()]).await?
        {
            ensure!(
                row.try_get::<String>("", "principal")? == user.id
                    && row.try_get::<String>("", "agent")? == agent
                    && row.try_get::<String>("", "request_hash")? == hash,
                "This automation id belongs to a different request"
            );
            let data = super::super::event_triggers::present(&tx, user, w, agent,
                super::super::event_access::record(&tx, w, agent, &input.id).await?, &[], true).await?;
            tx.commit().await?;
            return Ok(data);
        }
        super::connect::may_change(&tx, user, w, agent).await?;
        super::connect::saved(&tx, w, agent, &input.system).await?;
        let on: bool = query(
            &tx,
            "SELECT events_on FROM relay_connected_systems WHERE workspace=$1 AND connection=$2",
            vec![w.into(), input.system.clone().into()],
        )
        .await?
        .context("System")?
        .try_get("", "events_on")?;
        ensure!(on, "Turn on this system's events first");
        let read = read_tool(&tx, w, agent, &input.system).await?;
        let source = source(&tx, user, w, input, &read).await?;
        ensure!(
            input
                .filter
                .as_object()
                .is_some_and(|f| f.keys().all(|k| input.fields.contains(k))),
            "Filter only on the fields the agent sees"
        );
        let authority = super::super::event_access::versions(
            &tx,
            user,
            w,
            agent,
            input.chat.as_deref(),
            &source,
        )
        .await?;
        let trigger = input.trigger(
            &source.id,
            &source.fingerprint()?,
            authority["connection_revision"]
                .as_i64()
                .context("Revision")?,
            authority["read_revision"].as_i64().context("Revision")?,
        );
        super::super::event_triggers::insert(&tx, user, w, agent, &trigger, &source, &hash).await?;
        execute(&tx, "INSERT INTO relay_system_automations(workspace,trigger_id,connection,results) VALUES($1,$2,$3,$4)",
            vec![w.into(), input.id.clone().into(), input.system.clone().into(), serde_json::to_value(&input.results)?.into()]).await?;
        let data = super::super::event_triggers::present(
            &tx,
            user,
            w,
            agent,
            super::super::event_access::record(&tx, w, agent, &input.id).await?,
            std::slice::from_ref(&source),
            true,
        )
        .await?;
        tx.commit().await?;
        Ok(data)
    }
}

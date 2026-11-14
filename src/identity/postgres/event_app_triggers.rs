//! Triggers started by a connected app's event (schema 109). Authority is the
//! same as for an operator source: the person must be able to ask the agent in
//! the destination, and the agent needs an effective Allow on the app's read
//! action there (see `connectors::triggers::read_operation`). Provider calls
//! happen outside these transactions, in `events::subscriptions`.
use super::super::event_access::{expected, record, versions};
use super::super::event_triggers::{destinations, present};
use super::super::*;
use super::{row, source_from};
use crate::identity::connectors::triggers::{self, AppEvent};
use crate::identity::events::{
    catalog::Source,
    model::{FromApp, fingerprint},
};
use serde_json::json;

/// A checked request, before any provider call.
pub(crate) struct AppPlan {
    /// The active subscription the trigger will share, if there is one.
    pub existing: Option<Source>,
    pub event: AppEvent,
    pub provider: String,
    pub server: String,
    pub binding: Value,
    pub fingerprint: String,
}

/// A subscription made at the provider, saved together with its first trigger.
pub(crate) struct Created {
    pub id: String,
    pub provider_id: String,
    /// The provider's signing key, already sealed.
    pub signing_key: Option<Vec<u8>>,
}

/// The saved connection an app event comes from.
struct Account {
    server: String,
    provider: String,
    binding: Value,
    fingerprint: String,
    definition: Value,
    reconnect: bool,
}

async fn account(db: &impl ConnectionTrait, w: &str, connection: &str) -> Result<Account> {
    let row = query(db, "SELECT c.server,c.provider,c.binding,c.fingerprint,c.definition,EXISTS(SELECT 1 FROM relay_connector_reconnects r WHERE r.workspace=c.workspace AND r.connection=c.id) AS reconnect FROM relay_control_connections c JOIN relay_control_systems s ON s.workspace=c.workspace AND s.server=c.server WHERE c.workspace=$1 AND c.id=$2 AND c.enabled AND s.approved",
        vec![w.into(), connection.into()]).await?.context("This app account is not connected")?;
    Ok(Account {
        server: row.try_get("", "server")?,
        provider: row.try_get("", "provider")?,
        binding: row
            .try_get::<Option<Value>>("", "binding")?
            .unwrap_or(Value::Null),
        fingerprint: row.try_get("", "fingerprint")?,
        definition: row.try_get("", "definition")?,
        reconnect: row.try_get("", "reconnect")?,
    })
}

/// The source a subscription of `event` on this account has, or will have.
fn source_for(
    w: &str,
    connection: &str,
    account: &Account,
    event: &AppEvent,
    read: &str,
    id: &str,
    provider_id: &str,
) -> Result<Source> {
    source_from(&json!({"workspace":w,"id":id,"connection":connection,
        "event":event.id,"provider":account.provider,"provider_id":provider_id,"trigger":event.trigger,
        "fields":event.map,"binding":account.binding,"connection_fingerprint":account.fingerprint,
        "read_operation":read,"name":format!("{} · {}", event.title, event.app_name),"state":"active"}))
}

async fn active(
    db: &impl ConnectionTrait,
    w: &str,
    connection: &str,
    event: &AppEvent,
) -> Result<Option<Source>> {
    row(db, "s.workspace=$1 AND s.connection=$2 AND s.event=$3 AND s.config_hash=$4 AND s.state='active'",
        vec![w.into(), connection.into(), event.id.clone().into(), fingerprint(&event.config)?.into()])
        .await?
        .map(|r| source_from(&r))
        .transpose()
}

/// Everything that can be checked without the provider, as at activation.
async fn check(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    agent: &str,
    input: &FromApp,
) -> Result<(AppPlan, Account)> {
    input.validate()?;
    let account = account(tx, w, &input.connection).await?;
    ensure!(
        !account.reconnect,
        "This app account needs reconnecting before its events can start work. An admin can reconnect it on Apps."
    );
    let event = triggers::event(&account.server, &account.provider, &input.event)
        .context("This app doesn't offer that event")?;
    let read = triggers::read_operation(&account.server, &account.definition)
        .context("This account offers no read action, so its events can't start an agent")?;
    super::super::automation_access::run_authority(tx, user, w, agent, None, input.chat.as_deref())
        .await?;
    let probe = source_for(
        w,
        &input.connection,
        &account,
        &event,
        &read,
        &input.id,
        "pending",
    )?;
    let trigger = input.trigger(&probe.id, &probe.fingerprint()?);
    ensure!(
        versions(tx, user, w, agent, input.chat.as_deref(), &probe).await? == expected(&trigger),
        "Read authority changed; refresh before activation"
    );
    ensure!(
        input
            .filter
            .as_object()
            .is_some_and(|f| f.keys().all(|k| event.map.contains_key(k))),
        "Filter only the event's fields"
    );
    let plan = AppPlan {
        existing: active(tx, w, &input.connection, &event).await?,
        event,
        provider: account.provider.clone(),
        server: account.server.clone(),
        binding: account.binding.clone(),
        fingerprint: account.fingerprint.clone(),
    };
    Ok((plan, account))
}

/// The saved trigger for an exact repeat of `input`.
async fn replay(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    agent: &str,
    input: &FromApp,
) -> Result<Option<Value>> {
    let Some(row) = query(tx, "SELECT principal,agent,request_hash FROM relay_event_triggers WHERE workspace=$1 AND id=$2", vec![w.into(), input.id.clone().into()]).await? else {
        return Ok(None);
    };
    ensure!(
        row.try_get::<String>("", "principal")? == user.id
            && row.try_get::<String>("", "agent")? == agent
            && row.try_get::<String>("", "request_hash")? == fingerprint(input)?,
        "Event-trigger ID belongs to a different request"
    );
    Ok(Some(
        present(
            tx,
            user,
            w,
            agent,
            record(tx, w, agent, &input.id).await?,
            &[],
            true,
        )
        .await?,
    ))
}

impl PgStore {
    /// An exact earlier save of `input`, or the checked plan for a new one.
    /// Nothing is written.
    pub(crate) async fn app_trigger_plan(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        input: &FromApp,
    ) -> Result<std::result::Result<Value, AppPlan>> {
        let tx = self.read_tx(w).await?;
        if let Some(saved) = replay(&tx, user, w, agent, input).await? {
            tx.commit().await?;
            return Ok(Ok(saved));
        }
        ensure!(
            self.pooled,
            "Event work requires pooled background execution"
        );
        let (plan, _) = check(&tx, user, w, agent, input).await?;
        tx.commit().await?;
        Ok(Err(plan))
    }

    /// Saves the trigger, and the subscription `created` at the provider when
    /// there is no active one to share, in one transaction: never one without
    /// the other. Returns the trigger and a provider subscription id that ended
    /// up unused (a concurrent request saved its own first), to remove.
    pub(crate) async fn save_app_trigger(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        input: &FromApp,
        created: Option<&Created>,
    ) -> Result<(Value, Option<String>)> {
        let tx = self.workspace_tx(w).await?;
        if let Some(saved) = replay(&tx, user, w, agent, input).await? {
            tx.commit().await?;
            return Ok((saved, created.map(|c| c.provider_id.clone())));
        }
        ensure!(
            self.pooled,
            "Event work requires pooled background execution"
        );
        let (plan, account) = check(&tx, user, w, agent, input).await?;
        let (source, unused) = match (plan.existing, created) {
            (Some(existing), created) => {
                let unused = created
                    .filter(|c| c.provider_id != existing.trigger_id)
                    .map(|c| c.provider_id.clone());
                (existing, unused)
            }
            (None, Some(c)) => {
                let read = triggers::read_operation(&account.server, &account.definition)
                    .context("This account offers no read action")?;
                let source = source_for(
                    w,
                    &input.connection,
                    &account,
                    &plan.event,
                    &read,
                    &c.id,
                    &c.provider_id,
                )?;
                execute(&tx, "INSERT INTO relay_connector_subscriptions(workspace,id,connection,event,provider,provider_id,trigger,config,config_hash,fields,binding,connection_fingerprint,read_operation,name,signing_key,created_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)",
                    vec![w.into(), c.id.clone().into(), input.connection.clone().into(), plan.event.id.clone().into(), account.provider.clone().into(), c.provider_id.clone().into(), plan.event.trigger.clone().into(), plan.event.config.clone().into(), fingerprint(&plan.event.config)?.into(), json!(plan.event.map).into(), account.binding.clone().into(), account.fingerprint.clone().into(), read.into(), source.name.clone().into(), c.signing_key.clone().into(), user.id.clone().into()]).await?;
                audit(
                    &tx,
                    &user.id,
                    Some(w),
                    "connector_subscription_created",
                    &c.id,
                )
                .await?;
                (source, None)
            }
            (None, None) => anyhow::bail!("The app event changed while saving; try again"),
        };
        let trigger = input.trigger(&source.id, &source.fingerprint()?);
        super::super::event_triggers::insert(
            &tx,
            user,
            w,
            agent,
            &trigger,
            &source,
            &fingerprint(input)?,
        )
        .await?;
        let data = present(
            &tx,
            user,
            w,
            agent,
            record(&tx, w, agent, &input.id).await?,
            std::slice::from_ref(&source),
            true,
        )
        .await?;
        tx.commit().await?;
        Ok((data, unused))
    }

    /// Connected apps whose events could start this agent, each with what
    /// stops it, if anything. `unavailable` says why a provider's events can't
    /// be delivered on this server, or `None` when they can.
    pub(crate) async fn app_event_choices(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        unavailable: &(dyn Fn(&str) -> Option<String> + Sync),
    ) -> Result<Vec<Value>> {
        let tx = self.read_tx(w).await?;
        super::super::agent_profiles::managed_access(&tx, &user.id, w, agent, true).await?;
        let ids = rows(&tx, "SELECT c.id FROM relay_control_connections c JOIN relay_control_systems s ON s.workspace=c.workspace AND s.server=c.server WHERE c.workspace=$1 AND c.enabled AND s.approved ORDER BY c.server,c.id LIMIT 50", vec![w.into()]).await?;
        let mut choices = vec![];
        for row in ids {
            let id: String = row.try_get("", "id")?;
            let account = account(&tx, w, &id).await?;
            let events = triggers::events(&account.server, &account.provider);
            let Some(first) = events.first() else {
                continue;
            };
            let name = crate::identity::connectors::curated::app(&account.server)
                .map(|a| a.name.clone())
                .unwrap_or_else(|| account.server.clone());
            let mut choice = json!({"connection":id,"app":account.server,"name":name,
                "account":account.definition["account"],"state":"ready","detail":null,"conversations":[]});
            let read = triggers::read_operation(&account.server, &account.definition);
            let why = if account.reconnect {
                Some(("reconnect", "This account needs reconnecting before its events can start work. An admin can reconnect it on Apps.".to_owned()))
            } else if let Some(why) = unavailable(&account.provider) {
                Some(("unavailable", why))
            } else if read.is_none() {
                Some((
                    "no_read",
                    "This account offers no read action, so its events can't start an agent."
                        .to_owned(),
                ))
            } else {
                None
            };
            if let Some((state, detail)) = why {
                choice["state"] = json!(state);
                choice["detail"] = json!(detail);
                choices.push(choice);
                continue;
            }
            let read = read.unwrap_or_default();
            let probe = source_for(
                w,
                &id,
                &account,
                first,
                &read,
                &uuid::Uuid::new_v4().to_string(),
                "pending",
            )?;
            let (conversations, authority) = destinations(&tx, user, w, agent, &probe).await?;
            let title = account.definition["operations"]
                .as_array()
                .and_then(|ops| ops.iter().find(|o| o["id"] == read.as_str()))
                .and_then(|o| o["title"].as_str())
                .unwrap_or("its read action")
                .to_owned();
            // Runs happen in a work room (schema 115); a conversation is only
            // an optional place to post results.
            if authority.is_null() {
                choice["state"] = json!("needs_allow");
                choice["detail"] = json!(format!(
                    "Set \u{201c}{title}\u{201d} to Allow on this agent\u{2019}s Tools tab."
                ));
            }
            choice["read_operation"] = json!(read);
            choice["read_title"] = json!(title);
            choice["conversations"] = json!(conversations);
            choice["authority"] = authority;
            choices.push(choice);
        }
        tx.commit().await?;
        Ok(choices)
    }

    /// `GET /apps/{connection}/events`: the app's events under Cuckoo's names,
    /// for any member (everyone may see connected apps).
    pub(crate) async fn app_events(&self, user: &User, w: &str, connection: &str) -> Result<Value> {
        crate::identity::automations::model::uuid(connection)?;
        let tx = self.read_tx(w).await?;
        role(&tx, w, &user.id).await?;
        let account = account(&tx, w, connection).await?;
        tx.commit().await?;
        let events: Vec<Value> = triggers::events(&account.server, &account.provider)
            .iter()
            .map(AppEvent::public)
            .collect();
        Ok(json!({"events":events}))
    }
}

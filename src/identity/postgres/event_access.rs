use super::*;
use crate::identity::events::{
    catalog::{self, Source},
    model::Trigger,
};
use serde_json::json;
pub(super) async fn exists(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regclass('relay_event_deliveries') IS NOT NULL AS present",
        vec![],
    )
    .await?
    .unwrap()
    .try_get("", "present")?)
}
pub(super) async fn versions(
    db: &impl ConnectionTrait,
    user: &User,
    w: &str,
    agent: &str,
    chat: &str,
    source: &Source,
) -> Result<Value> {
    ensure!(source.workspace == w, "Event source workspace mismatch");
    let row=query(db,"SELECT c.revision AS connection_revision,r.revision AS read_revision,c.definition FROM relay_control_connections c JOIN relay_control_effective_rules r ON r.workspace=c.workspace AND r.connection=c.id WHERE c.workspace=$1 AND c.id=$2 AND c.enabled AND c.fingerprint=$3 AND r.agent=$4 AND r.chat=$5 AND r.operation=$6 AND r.mode='allow'",vec![w.into(),source.connection.clone().into(),source.connection_fingerprint.clone().into(),agent.into(),chat.into(),source.read_operation.clone().into()]).await?.context("This destination needs an existing Allow rule for the reviewed source read operation")?;
    let definition: Value = row.try_get("", "definition")?;
    ensure!(
        definition["operations"].as_array().is_some_and(|ops| ops
            .iter()
            .any(|op| op["id"] == source.read_operation && op["effect"] == "read")),
        "The source operation must be a reviewed read"
    );
    super::chat::access(db, user, w, chat).await?;
    let delegation = if role(db, w, &user.id).await? == "Owner" {
        None
    } else {
        Some(query(db,"SELECT d.revision FROM relay_control_delegations d JOIN relay_members m ON m.workspace=d.workspace AND m.user_id=d.approver AND m.role='Owner' WHERE d.workspace=$1 AND d.connection=$2 AND d.operation=$3 AND d.user_id=$4 AND d.ceiling='allow'",vec![w.into(),source.connection.clone().into(),source.read_operation.clone().into(),user.id.clone().into()]).await?.context("An Owner must delegate automatic source reading before you can configure event work")?.try_get::<i64>("","revision")?)
    };
    Ok(
        json!({"connection_revision":row.try_get::<i64>("","connection_revision")?,"read_revision":row.try_get::<i64>("","read_revision")?,"delegation_revision":delegation}),
    )
}
pub(super) fn expected(input: &Trigger) -> Value {
    json!({"connection_revision":input.connection_revision,"read_revision":input.read_revision,"delegation_revision":input.delegation_revision})
}
pub(super) async fn record(
    db: &impl ConnectionTrait,
    w: &str,
    agent: &str,
    id: &str,
) -> Result<Value> {
    query(db,"SELECT to_jsonb(t)-'request_hash' AS data FROM relay_event_triggers t WHERE workspace=$1 AND agent=$2 AND id=$3",vec![w.into(),agent.into(),id.into()]).await?.context("Event trigger unavailable")?.try_get("","data").map_err(Into::into)
}
pub(super) async fn visible(
    db: &impl ConnectionTrait,
    user: &User,
    w: &str,
    agent: &str,
    data: &Value,
) -> Result<()> {
    super::agent_profiles::managed_access(db, &user.id, w, agent, true).await?;
    super::chat::access(
        db,
        user,
        w,
        data["chat"].as_str().context("Missing event destination")?,
    )
    .await
}
pub(super) fn principal(data: &Value) -> Result<User> {
    Ok(User {
        id: data["principal"]
            .as_str()
            .context("Missing event principal")?
            .into(),
        email: String::new(),
        display_name: None,
    })
}
pub(super) async fn current(db: &impl ConnectionTrait, w: &str, data: &Value) -> Result<Value> {
    let source = catalog::source(w, data["source"].as_str().context("Missing event source")?)?;
    ensure!(
        source.fingerprint()? == data["binding"],
        "Event-source identity or projection changed; create a new trigger"
    );
    versions(
        db,
        &principal(data)?,
        w,
        data["agent"].as_str().unwrap(),
        data["chat"].as_str().unwrap(),
        &source,
    )
    .await
}
pub(super) async fn scope(
    db: &impl ConnectionTrait,
    w: &str,
    reply: &str,
) -> Result<Option<String>> {
    if !exists(db).await? {
        return Ok(None);
    }
    let handoff = if super::handoff_access::exists(db).await? {
        " UNION ALL SELECT source->>'source' AS source FROM relay_agent_handoffs WHERE workspace=$1 AND (child=$2 OR root=$2 OR request_message=$2) AND source IS NOT NULL"
    } else {
        ""
    };
    let sql = format!(
        "SELECT source FROM relay_event_deliveries WHERE workspace=$1 AND reply=$2{handoff} LIMIT 1"
    );
    query(db, &sql, vec![w.into(), reply.into()])
        .await?
        .map(|r| r.try_get("", "source"))
        .transpose()
        .map_err(Into::into)
}
pub(super) async fn allowed(
    db: &impl ConnectionTrait,
    job: &crate::identity::chat::store::Job,
) -> Result<bool> {
    if !exists(db).await? {
        return Ok(true);
    }
    let data=query(db,"SELECT to_jsonb(t)-'request_hash' AS data,d.authority,d.revoked FROM relay_event_deliveries d JOIN relay_event_triggers t ON t.workspace=d.workspace AND t.id=d.trigger_id WHERE d.workspace=$1 AND d.reply=$2",vec![job.workspace.clone().into(),job.reply.clone().into()]).await?;
    let Some(row) = data else { return Ok(true) };
    if row.try_get::<bool>("", "revoked")? {
        return Ok(false);
    }
    let data: Value = row.try_get("", "data")?;
    if data["chat"] != job.chat || data["agent"] != job.agent {
        return Ok(false);
    }
    let user = User {
        id: job.requester.clone(),
        email: String::new(),
        display_name: None,
    };
    if super::chat::access(db, &user, &job.workspace, &job.chat)
        .await
        .is_err()
    {
        return Ok(false);
    }
    match current(db, &job.workspace, &data).await {
        Ok(current) => Ok(current == row.try_get::<Value>("", "authority")?),
        Err(error) if error.is::<sea_orm::DbErr>() => Err(error),
        Err(_) => Ok(false),
    }
}

/// Recompute the bounded active queue after composing all current source guards.
pub(super) async fn pending_counts(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    profiles: &mut [Value],
) -> Result<()> {
    if !exists(tx).await? {
        return Ok(());
    }
    let mut counts = std::collections::HashMap::<String, i64>::new();
    for row in rows(tx,"SELECT j.payload FROM relay_chat_jobs j JOIN relay_chats c ON c.workspace=j.workspace AND c.id=j.payload->>'chat' WHERE j.workspace=$1 AND j.state IN ('queued','running','waiting') AND (c.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id=$2))",vec![w.into(),user.id.clone().into()]).await? {
        let mut job:crate::identity::chat::store::Job=serde_json::from_value(row.try_get("","payload")?)?;
        job.requester=user.id.clone();
        if super::source_access::allowed(tx,&job).await? {*counts.entry(job.agent).or_default()+=1;}
    }
    for profile in profiles {
        profile["pending"] = json!(
            counts
                .get(profile["id"].as_str().unwrap())
                .copied()
                .unwrap_or(0)
        );
    }
    Ok(())
}

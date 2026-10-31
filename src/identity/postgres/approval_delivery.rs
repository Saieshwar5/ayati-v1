//! Channel records are references. Exact private details are projected only after current access checks.
use super::*;
use serde_json::json;

pub(super) async fn action(db: &impl ConnectionTrait, w: &str, id: &str) -> Result<Value> {
    query(
        db,
        "SELECT to_jsonb(a) AS data FROM relay_control_actions a WHERE workspace=$1 AND id=$2",
        vec![w.into(), id.into()],
    )
    .await?
    .context("Action unavailable")?
    .try_get("", "data")
    .map_err(Into::into)
}
pub(super) async fn viewer(
    db: &impl ConnectionTrait,
    user: &User,
    w: &str,
    a: &Value,
) -> Result<()> {
    let agent = a["agent"].as_str().context("Missing agent")?;
    super::approval_authority::agent_visible(db, &user.id, w, agent).await?;
    super::chat::access(db, user, w, a["chat"].as_str().unwrap()).await?;
    let mut job = super::control_actions::job_for_action(db, a).await?;
    job.requester = user.id.clone();
    ensure!(
        super::source_access::allowed(db, &job).await?,
        "Approval source data is unavailable to this person"
    );
    Ok(())
}
pub(super) async fn decision_authority(
    db: &impl ConnectionTrait,
    user: &User,
    w: &str,
    a: &Value,
) -> Result<()> {
    viewer(db, user, w, a).await?;
    let job = super::control_actions::job_for_action(db, a).await?;
    ensure!(
        super::handoff_controls::allowed(
            db,
            &job,
            a["connection"].as_str().unwrap(),
            a["operation"].as_str().unwrap()
        )
        .await?,
        "Handoff operation ceiling changed"
    );
    let rule=query(db,"SELECT revision FROM relay_control_effective_rules WHERE workspace=$1 AND agent=$2 AND connection=$3 AND operation=$4 AND mode='ask'",vec![w.into(),a["agent"].as_str().unwrap().into(),a["connection"].as_str().unwrap().into(),a["operation"].as_str().unwrap().into()]).await?.context("Approval authority changed")?;
    ensure!(
        rule.try_get::<i64>("", "revision")? == a["rule_revision"].as_i64().unwrap(),
        "Approval authority changed"
    );
    super::approval_authority::may_decide(db, user, w, a).await
}
pub(super) async fn details(
    db: &impl ConnectionTrait,
    user: &User,
    w: &str,
    a: &Value,
) -> Result<Value> {
    viewer(db, user, w, a).await?;
    let id = a["id"].as_str().unwrap();
    let mut value = super::control_actions::visible(db, w, id).await?;
    let names=query(db,"SELECT c.definition->>'name' AS account,a.name,COALESCE(u.display_name,u.email) AS approver FROM relay_control_connections c JOIN relay_shared_agents a ON a.workspace=c.workspace AND a.id=$3 JOIN relay_users u ON u.id=$4 WHERE c.workspace=$1 AND c.id=$2",vec![w.into(),a["connection"].as_str().unwrap().into(),a["agent"].as_str().unwrap().into(),a["approver"].as_str().unwrap().into()]).await?.context("Approval context unavailable")?;
    value["source_scope"] =
        json!(super::event_access::scope(db, w, a["job"].as_str().unwrap()).await?);
    value["agent"] = a["agent"].clone();
    value["agent_name"] = json!(names.try_get::<String>("", "name")?);
    value["account_name"] = json!(names.try_get::<String>("", "account")?);
    value["approver_name"] = json!(names.try_get::<String>("", "approver")?);
    // "Waiting for Ravi or Meera", and when admins may step in.
    value["deciders"] = json!(super::approval_authority::deciders(db, w, a).await?);
    value["backup_at"] = json!(super::approval_authority::backup_at(db, w, a).await?);
    value["needs_other"] = a["needs_other"].clone();
    value["decided_by_name"] = match a["decided_by"].as_str() {
        Some(id) => json!(super::member_handover::name(db, id).await.ok()),
        None => Value::Null,
    };
    value["can_decide"] =
        json!(a["state"] == "pending" && decision_authority(db, user, w, a).await.is_ok());
    Ok(value)
}
pub(super) async fn deliver(db: &impl ConnectionTrait, w: &str, id: &str) -> Result<()> {
    let a = action(db, w, id).await?;
    // The request appears where it was asked, for the requester and every
    // manager in that conversation. When no manager can see it, the record says
    // so; an admin may then decide (approval_authority::may_decide).
    let target = a["chat"].as_str().unwrap();
    if !super::approval_authority::deciders(db, w, &a)
        .await?
        .is_empty()
    {
        let message = uuid::Uuid::new_v4().to_string();
        let thread = a["thread"].as_str();
        execute(db,"INSERT INTO relay_chat_messages(id,workspace,chat,author,kind,content,state,at,thread) VALUES($1,$2,$3,$4,'agent','Approval request','complete',$5,$6)",vec![message.clone().into(),w.into(),target.into(),a["agent"].as_str().unwrap().into(),now().into(),thread.into()]).await?;
        execute(db,"INSERT INTO relay_control_deliveries(workspace,action,chat,message,status) VALUES($1,$2,$3,$4,'channel')",vec![w.into(),id.into(),target.into(),message.into()]).await?;
    } else {
        execute(db,"INSERT INTO relay_control_deliveries(workspace,action,status,reason) VALUES($1,$2,'inbox','None of this agent''s managers can see this conversation, so an admin who can see it may decide.')",vec![w.into(),id.into()]).await?;
    }
    Ok(())
}
pub(super) async fn hydrate(
    db: &impl ConnectionTrait,
    user: &User,
    w: &str,
    messages: &mut [Value],
) -> Result<()> {
    let ids: Vec<_> = messages.iter().map(|m| m["id"].clone()).collect();
    let deliveries=rows(db,"SELECT message,action FROM relay_control_deliveries WHERE workspace=$1 AND message IN (SELECT jsonb_array_elements_text($2))",vec![w.into(),json!(ids).into()]).await?;
    for row in deliveries {
        let id: String = row.try_get("", "action")?;
        let message: String = row.try_get("", "message")?;
        if let Some(m) = messages.iter_mut().find(|m| m["id"] == message) {
            let a = action(db, w, &id).await?;
            m["approval"] = if viewer(db, user, w, &a).await.is_ok() {
                json!({"id":id,"state":super::control_actions::visible(db,w,&id).await?["state"]})
            } else {
                json!({"restricted":true})
            };
        }
    }
    Ok(())
}
impl PgStore {
    pub(crate) async fn approval_details(&self, user: &User, w: &str, id: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let a = action(&tx, w, id).await?;
        let value = details(&tx, user, w, &a).await?;
        tx.commit().await?;
        Ok(value)
    }
    pub(crate) async fn approval_inbox(&self, user: &User, w: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        role(&tx, w, &user.id).await?;
        let clause = super::approval_authority::decider_clause(&tx).await?;
        let actions=json_rows(&tx,&format!("SELECT to_jsonb(a) AS data FROM relay_control_actions a WHERE a.workspace=$1 AND {clause} AND (a.decision IS NOT NULL OR a.state='pending') ORDER BY (a.state='pending') DESC,a.created DESC,a.id DESC LIMIT 50"),vec![w.into(),user.id.clone().into()]).await?;
        let mut result = Vec::new();
        for a in actions {
            if let Ok(value) = details(&tx, user, w, &a).await {
                result.push(value);
            }
        }
        tx.commit().await?;
        Ok(json!({"actions":result}))
    }
}

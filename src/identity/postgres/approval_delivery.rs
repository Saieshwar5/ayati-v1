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
    ensure!(
        a["approver"] == user.id,
        "Only the designated authorized person can decide this action"
    );
    let rule=query(db,"SELECT approver,approver_revision,revision FROM relay_control_effective_rules WHERE workspace=$1 AND agent=$2 AND connection=$3 AND operation=$4 AND chat=$5 AND mode='ask'",vec![w.into(),a["agent"].as_str().unwrap().into(),a["connection"].as_str().unwrap().into(),a["operation"].as_str().unwrap().into(),a["chat"].as_str().unwrap().into()]).await?.context("Approval authority changed")?;
    ensure!(
        rule.try_get::<i64>("", "revision")? == a["rule_revision"].as_i64().unwrap()
            && rule.try_get::<String>("", "approver")? == user.id,
        "Approval authority changed"
    );
    super::approval_authority::require(
        db,
        w,
        a["connection"].as_str().unwrap(),
        a["operation"].as_str().unwrap(),
        &user.id,
        rule.try_get("", "approver_revision")?,
    )
    .await
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
    value["can_decide"] =
        json!(a["state"] == "pending" && decision_authority(db, user, w, a).await.is_ok());
    Ok(value)
}
pub(super) async fn deliver(db: &impl ConnectionTrait, w: &str, id: &str) -> Result<()> {
    let a = action(db, w, id).await?;
    let person = User {
        id: a["approver"].as_str().unwrap().into(),
        email: String::new(),
        display_name: None,
    };
    let source = a["chat"].as_str().unwrap();
    let target = a["approval_chat"].as_str().unwrap_or(source);
    let safe = viewer(db, &person, w, &a).await.is_ok()
        && super::approval_authority::route(
            db,
            &person,
            w,
            a["agent"].as_str().unwrap(),
            source,
            target,
            &person.id,
        )
        .await
        .is_ok();
    if safe {
        let message = uuid::Uuid::new_v4().to_string();
        let thread = if source == target {
            a["thread"].as_str()
        } else {
            None
        };
        execute(db,"INSERT INTO relay_chat_messages(id,workspace,chat,author,kind,content,state,at,thread) VALUES($1,$2,$3,$4,'agent','Approval request','complete',$5,$6)",vec![message.clone().into(),w.into(),target.into(),a["agent"].as_str().unwrap().into(),now().into(),thread.into()]).await?;
        execute(db,"INSERT INTO relay_control_deliveries(workspace,action,chat,message,status) VALUES($1,$2,$3,$4,'channel')",vec![w.into(),id.into(),target.into(),message.into()]).await?;
    } else {
        execute(db,"INSERT INTO relay_control_deliveries(workspace,action,status,reason) VALUES($1,$2,'inbox','The configured audience or approver access needs review. Details are available only in the restricted approval view when authorized.')",vec![w.into(),id.into()]).await?;
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
        let actions=json_rows(&tx,"SELECT to_jsonb(a) AS data FROM relay_control_actions a WHERE workspace=$1 AND approver=$2 AND (decision IS NOT NULL OR state='pending') ORDER BY (state='pending') DESC,created DESC,id DESC LIMIT 50",vec![w.into(),user.id.clone().into()]).await?;
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

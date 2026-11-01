use super::*;
use serde_json::json;
pub(super) async fn snapshot(
    db: &impl ConnectionTrait,
    user: &User,
    w: &str,
    id: &str,
    revision: i64,
) -> Result<Option<Value>> {
    let Some(row)=query(db,"SELECT s.definition,s.provenance,s.saved_ms,s.checked_ms,s.task,s.generation,s.what_changed,m.chat,m.thread,COALESCE((SELECT (to_jsonb(r)->>'direct')::boolean FROM relay_home_update_runs r WHERE r.workspace=s.workspace AND r.reply=s.task),false) AS direct FROM relay_home_snapshots s JOIN relay_chat_messages m ON m.workspace=s.workspace AND m.id=s.task WHERE s.workspace=$1 AND s.panel=$2 AND s.panel_revision=$3 ORDER BY s.generation DESC LIMIT 1",vec![w.into(),id.into(),revision.into()]).await? else {return Ok(None)};
    let provenance: Value = row.try_get("", "provenance")?;
    if !super::home_sources::allowed(db, w, &user.id, &provenance).await? {
        return Ok(Some(json!({"revision":revision,"available":false})));
    }
    Ok(Some(
        json!({"revision":revision,"available":true,"definition":row.try_get::<Value>("","definition")?,"saved_ms":row.try_get::<i64>("","saved_ms")?,"checked_ms":row.try_get::<i64>("","checked_ms")?,"generation":row.try_get::<i64>("","generation")?,"source_operations":provenance["operations"].as_array().map_or(0,Vec::len),"chat":row.try_get::<String>("","chat")?,"thread":row.try_get::<Option<String>>("","thread")?,"task":row.try_get::<String>("","task")?,"what_changed":row.try_get::<Option<String>>("","what_changed")?,"from_source":row.try_get::<bool>("","direct")?}),
    ))
}
pub(super) async fn maintenance(
    db: &DatabaseTransaction,
    user: &User,
    w: &str,
    id: &str,
    pooled: bool,
) -> Result<Value> {
    let p = super::home_maintenance::owned(db, user, w, id).await?;
    let scope = super::home_update_access::scopes(db, w, &p["provenance"]).await;
    let authority = super::home_maintenance::authorized(db, user, w, &p).await;
    let mut review = p.clone();
    review["maintenance"] = Value::Null;
    let request_authority = super::home_maintenance::authorized(db, user, w, &review).await;
    if let Err(error) = &request_authority
        && error.is::<sea_orm::DbErr>()
    {
        return Err(request_authority.unwrap_err());
    }
    if let Err(error) = &authority
        && error.is::<sea_orm::DbErr>()
    {
        return Err(authority.unwrap_err());
    }
    if let Err(error) = &scope
        && error.is::<sea_orm::DbErr>()
    {
        return Err(scope.unwrap_err());
    }
    let mut data = p["maintenance"].clone();
    if data.is_null() {
        data = json!({"revision":0,"enabled":false,"instructions":"","interval_seconds":null,"stale_seconds":3600,"panel_revision":p["active_revision"],"scopes":scope.as_ref().ok()});
    }
    data["eligible"] = json!(
        scope.is_ok()
            && request_authority.is_ok()
            && pooled
            && p["revision"] == p["active_revision"]
    );
    data["scopes"] = json!(scope.as_ref().ok());
    data["execution_available"] = json!(pooled);
    let last=query(db,"SELECT r.reply,r.created_ms,r.published_generation,j.state FROM relay_home_update_runs r JOIN relay_chat_jobs j ON j.workspace=r.workspace AND j.reply=r.reply WHERE r.workspace=$1 AND r.panel=$2 AND r.policy_revision=$3 ORDER BY r.created_ms DESC,r.reply DESC LIMIT 1",vec![w.into(),id.into(),data["revision"].as_i64().unwrap().into()]).await?;
    let saved=query(db,"SELECT checked_ms,saved_ms FROM relay_home_snapshots WHERE workspace=$1 AND panel=$2 AND panel_revision=$3 ORDER BY generation DESC LIMIT 1",vec![w.into(),id.into(),p["active_revision"].as_i64().unwrap().into()]).await?;
    let checked = saved
        .as_ref()
        .map(|r| r.try_get::<i64>("", "checked_ms"))
        .transpose()?;
    data["checked_ms"] = json!(checked);
    // Living panels: how many updates in the last day, and how many ran without the agent.
    let day = query(db, "SELECT count(*) AS updates,count(*) FILTER (WHERE to_jsonb(r)->>'direct'='true') AS direct FROM relay_home_update_runs r WHERE r.workspace=$1 AND r.panel=$2 AND r.created_ms>(extract(epoch FROM clock_timestamp())*1000)::bigint-86400000", vec![w.into(), id.into()]).await?.context("Update counts")?;
    data["last_day"] = json!({"updates": day.try_get::<i64>("", "updates")?, "without_agent": day.try_get::<i64>("", "direct")?});
    data["from_source"] = json!(!data["bindings"].is_null());
    data["saved_ms"] = json!(
        saved
            .map(|r| r.try_get::<i64>("", "saved_ms"))
            .transpose()?
    );
    let schedule = if let Some(sid) = data["schedule"].as_str() {
        query(
            db,
            "SELECT state,next_ms FROM relay_agent_schedules WHERE workspace=$1 AND id=$2",
            vec![w.into(), sid.into()],
        )
        .await?
    } else {
        None
    };
    data["next_ms"] = json!(
        schedule
            .as_ref()
            .map(|r| r.try_get::<Option<i64>>("", "next_ms"))
            .transpose()?
            .flatten()
    );
    let now = query(
        db,
        "SELECT (extract(epoch FROM clock_timestamp())*1000)::bigint AS now",
        vec![],
    )
    .await?
    .unwrap()
    .try_get::<i64>("", "now")?;
    let status = if p["revision"] != p["active_revision"]
        || (data["revision"] != 0 && data["panel_revision"] != p["active_revision"])
    {
        "review_required"
    } else if data["enabled"] != true {
        "paused"
    } else if authority.is_err()
        || schedule
            .as_ref()
            .is_some_and(|r| r.try_get::<String>("", "state").ok().as_deref() == Some("blocked"))
    {
        "blocked"
    } else if let Some(last) = last.as_ref() {
        let state: String = last.try_get("", "state")?;
        if state != "finished" {
            "updating"
        } else if last
            .try_get::<Option<i64>>("", "published_generation")?
            .is_none()
        {
            "failed"
        } else if checked.is_none_or(|at| now - at > data["stale_seconds"].as_i64().unwrap() * 1000)
        {
            "delayed"
        } else {
            "current"
        }
    } else {
        "awaiting_update"
    };
    data["status"] = json!(status);
    // The latest update, so a delayed or failed panel can open it in Activity.
    data["last_update"] = json!(
        last.as_ref()
            .map(|r| r.try_get::<String>("", "reply"))
            .transpose()?
    );
    Ok(data)
}

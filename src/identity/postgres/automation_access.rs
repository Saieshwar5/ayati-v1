use super::*;
use serde_json::json;
pub(super) async fn record(
    tx: &DatabaseTransaction,
    w: &str,
    agent: &str,
    id: &str,
) -> Result<Value> {
    query(tx,"SELECT to_jsonb(s)-'request_hash' AS data FROM relay_agent_schedules s WHERE workspace=$1 AND agent=$2 AND id=$3",vec![w.into(),agent.into(),id.into()]).await?.context("Routine unavailable")?.try_get("","data").map_err(Into::into)
}
pub(super) async fn visible(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    agent: &str,
    data: &Value,
) -> Result<()> {
    standing(tx, user, w, agent, data["id"].as_str()).await?;
    // A schedule that posts to a conversation is seen only by people in it.
    if let Some(chat) = data["chat"].as_str() {
        super::chat::access(tx, user, w, chat).await?;
    }
    Ok(())
}
/// What a schedule needs from its person to run: they may still look after
/// the agent and use it directly, and post where it posts, if anywhere. Runs
/// happen in the schedule's own work room (schema 56), not in a conversation.
///
/// `own` names the schedule when it may be the person's own: one they saved from the
/// agent's proposal in their chat (see `standing`).
pub(super) async fn run_authority(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    agent: &str,
    own: Option<&str>,
    post_to: Option<&str>,
) -> Result<()> {
    let profile = standing(tx, user, w, agent, own).await?;
    ensure!(
        profile["revision"].as_i64().is_some_and(|r| r > 0),
        "Use a managed agent for shared routines"
    );
    ensure!(
        super::chat::agent_access(tx, &user.id, w, agent).await?,
        "Current agent request permission is required"
    );
    match post_to {
        Some(chat) => authority_for(tx, user, w, agent, own, chat).await,
        None => Ok(()),
    }
}
/// Whether `user` stands behind schedules on the agent: they look after it, or
/// `own` is their own schedule, saved from the agent's proposal in their chat (the
/// draft's id is the schedule's), and they may still talk to the agent. Members
/// can't make one any other way.
pub(super) async fn standing(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    agent: &str,
    own: Option<&str>,
) -> Result<Value> {
    match super::agent_profiles::managed_access(tx, &user.id, w, agent, true).await {
        Err(error) if !error.is::<sea_orm::DbErr>() => match own {
            Some(id) if proposed_to(tx, w, agent, id, &user.id).await? => {
                super::agent_profiles::managed_access(tx, &user.id, w, agent, false).await
            }
            _ => Err(error),
        },
        result => result,
    }
}
/// The person saved (or is saving) this schedule from the agent's proposal to them.
async fn proposed_to(
    tx: &DatabaseTransaction,
    w: &str,
    agent: &str,
    id: &str,
    user: &str,
) -> Result<bool> {
    let present: bool = query(
        tx,
        "SELECT to_regclass('relay_schedule_drafts') IS NOT NULL AS present",
        vec![],
    )
    .await?
    .context("Schema check")?
    .try_get("", "present")?;
    Ok(present
        && query(tx, "SELECT 1 AS found FROM relay_schedule_drafts WHERE workspace=$1 AND id=$2 AND agent=$3 AND requester=$4 AND state IN ('pending','saved')",
            vec![w.into(), id.into(), agent.into(), user.into()]).await?.is_some())
}
pub(super) async fn authority(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    agent: &str,
    chat: &str,
) -> Result<()> {
    authority_for(tx, user, w, agent, None, chat).await
}
/// May post the agent's work in `chat` as `user`; `own` as in `standing`.
pub(super) async fn authority_for(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    agent: &str,
    own: Option<&str>,
    chat: &str,
) -> Result<()> {
    let profile = standing(tx, user, w, agent, own).await?;
    ensure!(
        profile["revision"].as_i64().is_some_and(|r| r > 0),
        "Use a managed agent for shared routines"
    );
    super::chat::access(tx, user, w, chat).await?;
    ensure!(
        super::chat::agent_access_in(tx, &user.id, w, agent, chat).await?,
        "Current agent request permission is required"
    );
    ensure!(query(tx,"SELECT a.agent FROM relay_chat_agents a JOIN relay_chats c ON c.workspace=a.workspace AND c.id=a.chat WHERE a.workspace=$1 AND a.chat=$2 AND a.agent=$3 AND (NOT c.announcements OR EXISTS(SELECT 1 FROM relay_members m WHERE m.workspace=$1 AND m.user_id=$4 AND m.role='Admin'))",vec![w.into(),chat.into(),agent.into(),user.id.clone().into()]).await?.is_some(),"Agent participation and destination posting authority are required");
    super::channel_lifecycle::accepting(tx, w, chat).await
}
pub(super) fn principal(data: &Value) -> Result<User> {
    Ok(User {
        id: data["principal"]
            .as_str()
            .context("Missing authorizing person")?
            .into(),
        email: String::new(),
        display_name: None,
    })
}
pub(super) async fn present(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    agent: &str,
    mut data: Value,
) -> Result<Value> {
    visible(tx, user, w, agent, &data).await?;
    let person = principal(&data)?;
    let names=query(tx,"SELECT (SELECT c.name FROM relay_chats c WHERE c.workspace=$1 AND c.id=$2) AS chat_name,COALESCE(u.display_name,u.email) AS person_name,EXISTS(SELECT 1 FROM relay_schedule_occurrences o JOIN relay_chat_jobs j ON j.workspace=o.workspace AND j.reply=o.reply WHERE o.workspace=$1 AND o.schedule=$4 AND j.state<>'finished') AS unfinished FROM relay_users u WHERE u.id=$3",vec![w.into(),data["chat"].as_str().map(str::to_owned).into(),person.id.clone().into(),data["id"].as_str().unwrap().into()]).await?.context("Routine source unavailable")?;
    data["chat_name"] = json!(names.try_get::<Option<String>>("", "chat_name")?);
    data["principal_name"] = json!(names.try_get::<String>("", "person_name")?);
    data["unfinished"] = json!(names.try_get::<bool>("", "unfinished")?);
    data["can_resume"] = json!(
        person.id == user.id
            && data["state"] != "completed"
            && run_authority(
                tx,
                user,
                w,
                agent,
                data["id"].as_str(),
                data["chat"].as_str()
            )
            .await
            .is_ok()
    );
    Ok(data)
}
// Called inside the same workspace mutation as source/grant changes. Regranting
// later cannot restart a routine which already lost its standing authority.
pub(super) async fn invalidate_inaccessible(tx: &DatabaseTransaction, w: &str) -> Result<()> {
    for data in json_rows(tx,"SELECT to_jsonb(s) AS data FROM relay_agent_schedules s WHERE workspace=$1 AND state='enabled'",vec![w.into()]).await? {
        let person=principal(&data)?;
        let result=if super::home_update_access::panel_schedule(tx,w,data["id"].as_str().unwrap()).await? {
            super::home_update_runs::schedule_authority(tx,&person,w,data["id"].as_str().unwrap()).await
        } else {run_authority(tx,&person,w,data["agent"].as_str().unwrap(),data["id"].as_str(),data["chat"].as_str()).await};
        if let Err(error)=result {
            if error.is::<sea_orm::DbErr>() { return Err(error); }
            execute(tx,"UPDATE relay_agent_schedules SET state='blocked',revision=revision+1,detail='Routine access or destination changed. The original person must review and resume.' WHERE workspace=$1 AND id=$2 AND state='enabled'",vec![w.into(),data["id"].as_str().unwrap().into()]).await?;
        }
    }
    Ok(())
}

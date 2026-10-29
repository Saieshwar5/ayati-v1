//! Only source-authorized channel projections may disclose handoff details.
use super::*;
use crate::identity::chat::model::Job;
use serde_json::json;
pub(super) async fn project(db: &impl ConnectionTrait, user: &User, job: &Job) -> Result<Value> {
    let mut viewer = job.clone();
    viewer.requester = user.id.clone();
    ensure!(
        super::source_access::allowed(db, &viewer).await?,
        "Handoff source unavailable"
    );
    if !super::handoff_access::exists(db).await? {
        return Ok(json!({"incoming":null,"outgoing":[]}));
    }
    let mut data=json_rows(db,"SELECT to_jsonb(h)-'input_hash'-'controls'-'source'||jsonb_build_object('sender_name',s.name,'target_name',a.name,'principal_name',COALESCE(u.display_name,u.email),'task_state',m.state,'job_state',j.state,'phase',m.progress->>'state') AS data FROM relay_agent_handoffs h JOIN relay_shared_agents s ON s.workspace=h.workspace AND s.id=h.sender JOIN relay_shared_agents a ON a.workspace=h.workspace AND a.id=h.target JOIN relay_users u ON u.id=h.principal JOIN relay_chat_jobs j ON j.workspace=h.workspace AND j.reply=h.child JOIN relay_chat_messages m ON m.workspace=h.workspace AND m.id=h.child WHERE h.workspace=$1 AND (h.parent=$2 OR h.child=$2) ORDER BY h.created_ms,h.id",vec![job.workspace.clone().into(),job.reply.clone().into()]).await?;
    let owner = role(db, &job.workspace, &user.id).await? == "Owner";
    for h in &mut data {
        h["can_stop"] = json!(
            (owner || h["principal"] == user.id)
                && h["job_state"] != "finished"
                && h["task_state"] == "queued"
        );
    }
    let incoming = data.iter().find(|h| h["child"] == job.reply).cloned();
    let outgoing = data
        .into_iter()
        .filter(|h| h["parent"] == job.reply)
        .collect::<Vec<_>>();
    Ok(json!({"incoming":incoming,"outgoing":outgoing}))
}
pub(super) async fn hydrate(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    messages: &mut [Value],
) -> Result<()> {
    if !super::handoff_access::exists(tx).await? {
        return Ok(());
    }
    let ids = messages.iter().map(|m| m["id"].clone()).collect::<Vec<_>>();
    for row in rows(tx,"SELECT h.request_message,j.payload FROM relay_agent_handoffs h JOIN relay_chat_jobs j ON j.workspace=h.workspace AND j.reply=h.child WHERE h.workspace=$1 AND h.request_message IN (SELECT jsonb_array_elements_text($2::jsonb))",vec![w.into(),json!(ids).into()]).await? {
  let request:String=row.try_get("","request_message")?;let mut job:Job=serde_json::from_value(row.try_get("","payload")?)?;job.requester=user.id.clone();
  if let Some(m)=messages.iter_mut().find(|m|m["id"]==request){
   m["source_scope"]=json!(super::event_access::scope(tx,w,&job.reply).await?);
   if !super::source_access::allowed(tx,&job).await?{m["content"]=json!("Handoff unavailable because source access changed.");m["response_unavailable"]=json!(true);continue}
   m["handoff"]=project(tx,user,&job).await?["incoming"].clone();
  }
 }
    Ok(())
}

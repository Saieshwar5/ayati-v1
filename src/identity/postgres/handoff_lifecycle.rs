use super::*;
use crate::identity::chat::store::Job;
pub(super) async fn invalidate_inaccessible(tx: &DatabaseTransaction, w: &str) -> Result<()> {
    if !super::handoff_access::exists(tx).await? {
        return Ok(());
    }
    for h in json_rows(tx,"SELECT to_jsonb(h) AS data FROM relay_agent_handoffs h WHERE workspace=$1 AND state='pending' ORDER BY depth,created_ms",vec![w.into()]).await? {
  let job=super::handoff_access::job(tx,w,h["child"].as_str().unwrap()).await?;
  if !super::source_access::allowed(tx,&job).await? {
   if let Some(scope)=h["source"]["source"].as_str() {query(tx,"SELECT relay_handoff_revoke_source($1,$2,$3,$4)",vec![w.into(),h["root"].as_str().unwrap().into(),scope.into(),job.chat.into()]).await?;continue}
  }else{
   let access=super::control_actions::authorize(tx,&job,false).await;
   if access.as_ref().is_err_and(|e|e.is::<sea_orm::DbErr>()){return Err(access.unwrap_err())}
   if access.is_ok(){continue}
  }
  query(tx,"SELECT relay_handoff_cancel_branch($1,$2,$3,'cancelled')",vec![w.into(),job.reply.into(),"Helper access changed; this delegation cannot be revived.".into()]).await?;
 }
    Ok(())
}
impl PgStore {
    pub(crate) async fn reconcile_handoffs(&self) -> Result<usize> {
        if !self.pooled {
            return Ok(0);
        }
        let now_ms = query(
            &self.db,
            "SELECT (extract(epoch FROM clock_timestamp())*1000)::bigint AS time",
            vec![],
        )
        .await?
        .unwrap()
        .try_get("", "time")?;
        self.reconcile_handoffs_at(now_ms).await
    }
    pub(super) async fn reconcile_handoffs_at(&self, now_ms: i64) -> Result<usize> {
        if !self.pooled {
            return Ok(0);
        }
        let roots=rows(&self.db,"SELECT workspace,root FROM relay_agent_handoffs WHERE state='pending' GROUP BY workspace,root ORDER BY min(checked_ms),workspace,root LIMIT 20",vec![]).await?;
        let mut n = 0;
        for row in roots {
            let w: String = row.try_get("", "workspace")?;
            let root: String = row.try_get("", "root")?;
            let tx = self.workspace_tx(&w).await?;
            execute(&tx,"UPDATE relay_agent_handoffs SET checked_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE workspace=$1 AND root=$2",vec![w.clone().into(),root.clone().into()]).await?;
            invalidate_inaccessible(&tx, &w).await?;
            for expired in rows(&tx,"SELECT child FROM relay_agent_handoffs WHERE workspace=$1 AND root=$2 AND state='pending' AND expires_ms<=$3 ORDER BY depth",vec![w.clone().into(),root.into(),now_ms.into()]).await? {
    query(&tx,"SELECT relay_handoff_cancel_branch($1,$2,$3,'expired')",vec![w.clone().into(),expired.try_get::<String>("","child")?.into(),"The saved helper wait deadline expired; no automatic retry was started.".into()]).await?;n+=1;
   }
            tx.commit().await?;
        }
        Ok(n)
    }
}
pub(super) async fn continuation(tx: &DatabaseTransaction, job: &Job) -> Result<Vec<Value>> {
    if !super::handoff_access::exists(tx).await? {
        return Ok(vec![]);
    }
    let mut result = vec![];
    for row in rows(tx,"SELECT to_jsonb(h) AS data,m.content FROM relay_agent_handoffs h JOIN relay_chat_messages m ON m.workspace=h.workspace AND m.id=h.child WHERE h.workspace=$1 AND h.parent=$2 ORDER BY h.created_ms,h.id",vec![job.workspace.clone().into(),job.reply.clone().into()]).await? {
  let h:Value=row.try_get("","data")?;ensure!(h["state"]!="pending","Helper task is still pending");
  let body:String=row.try_get("","content")?;let text=if h["state"]=="complete"{body.chars().take(2000).collect::<String>()}else{h["detail"].as_str().unwrap_or("Helper task did not complete.").into()};
  result.push(serde_json::json!({"id":h["id"],"key":h["request_key"],"agent":h["target"],"state":h["state"],"result":text,"truncated":h["state"]=="complete"&&body.chars().count()>2000,"chat":h["chat"],"thread":h["thread"],"reply":h["child"]}));
 }
    Ok(result)
}

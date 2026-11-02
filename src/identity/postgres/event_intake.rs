use super::*;
use crate::identity::events::{
    catalog::Source,
    model::{InboxFull, fingerprint, matches},
};
use serde_json::json;
impl PgStore {
    pub(crate) async fn accept_event(
        &self,
        source: &Source,
        id: &str,
        event: &Value,
        receipt_limit: i64,
    ) -> Result<Value> {
        ensure!(self.pooled, "Event intake requires pooled execution");
        source.validate()?;
        crate::identity::identifier(id)?;
        let data = source.project(id, event)?;
        let hash = fingerprint(event)?;
        let w = &source.workspace;
        let binding = source.fingerprint()?;
        let tx = self.workspace_tx(w).await?;
        if let Some(old) = query(
            &tx,
            "SELECT fingerprint FROM relay_event_inbox WHERE workspace=$1 AND source=$2 AND id=$3",
            vec![w.clone().into(), source.id.clone().into(), id.into()],
        )
        .await?
        {
            ensure!(
                old.try_get::<String>("", "fingerprint")? == hash,
                "Event ID belongs to different content"
            );
            tx.commit().await?;
            return Ok(json!({"accepted":true,"duplicate":true}));
        }
        // A signed source is still not permission to read an unapproved account.
        ensure!(query(&tx,"SELECT c.id FROM relay_control_connections c JOIN relay_control_systems s ON s.workspace=c.workspace AND s.server=c.server WHERE c.workspace=$1 AND c.id=$2 AND c.enabled AND s.approved AND c.fingerprint=$3",vec![w.clone().into(),source.connection.clone().into(),source.connection_fingerprint.clone().into()]).await?.is_some(),"Source account is no longer approved");
        let mut matching = vec![];
        for trigger in json_rows(&tx,"SELECT to_jsonb(t) AS data FROM relay_event_triggers t WHERE workspace=$1 AND source=$2 AND state='enabled'",vec![w.clone().into(),source.id.clone().into()]).await? {
            if trigger["binding"]!=binding {continue;}
            let authority=super::event_access::current(&tx,w,&trigger).await;
            let expected=json!({"connection_revision":trigger["connection_revision"],"read_revision":trigger["read_revision"],"delegation_revision":trigger["delegation_revision"]});
            if authority.as_ref().is_err_and(|e|e.is::<sea_orm::DbErr>()) {return Err(authority.unwrap_err());}
            let start=super::automation_access::authority(&tx,&super::event_access::principal(&trigger)?,w,trigger["agent"].as_str().unwrap(),trigger["chat"].as_str().unwrap()).await;
            if start.as_ref().is_err_and(|e|e.is::<sea_orm::DbErr>()) {return Err(start.unwrap_err());}
            if authority.ok().as_ref()!=Some(&expected) || start.is_err() {continue;}
            if matches(&trigger["input"]["filter"],&data) {matching.push((trigger,expected));}
        }
        let counts=query(&tx,"SELECT (SELECT count(*) FROM relay_event_inbox WHERE workspace=$1) AS receipts,(SELECT count(*) FROM relay_event_deliveries WHERE workspace=$1 AND state='pending') AS pending",vec![w.clone().into()]).await?.unwrap();
        if counts.try_get::<i64>("", "receipts")? >= receipt_limit
            || counts.try_get::<i64>("", "pending")? + matching.len() as i64 > 1000
        {
            return Err(InboxFull.into());
        }
        execute(&tx,"INSERT INTO relay_event_inbox(workspace,source,id,fingerprint,binding,data) VALUES($1,$2,$3,$4,$5,$6)",vec![w.clone().into(),source.id.clone().into(),id.into(),hash.into(),binding.into(),if matching.is_empty(){None}else{Some(data)}.into()]).await?;
        super::home_update_runs::living::on_event(&tx, w, &source.connection, id).await?;
        for (trigger, authority) in matching {
            execute(&tx,"INSERT INTO relay_event_deliveries(workspace,id,trigger_id,source,event_id,authority) VALUES($1,$2,$3,$4,$5,$6)",vec![w.clone().into(),uuid::Uuid::new_v4().to_string().into(),trigger["id"].as_str().unwrap().into(),source.id.clone().into(),id.into(),authority.into()]).await?;
        }
        tx.commit().await?;
        Ok(json!({"accepted":true,"duplicate":false}))
    }
    pub(crate) async fn event_job_context(
        &self,
        job: &crate::identity::chat::model::Job,
    ) -> Result<Vec<Value>> {
        let tx = self.read_tx(&job.workspace).await?;
        ensure!(
            super::source_access::allowed(&tx, job).await?,
            "Connected event source access changed"
        );
        let sources = if super::event_access::exists(&tx).await? {
            let handoff = if super::handoff_access::exists(&tx).await? {
                " OR d.id IN (SELECT source->>'delivery' FROM relay_agent_handoffs WHERE workspace=$1 AND child=$2 AND NOT source_revoked)"
            } else {
                ""
            };
            let sql = format!(
                "SELECT jsonb_build_object('kind','connected_event','id',d.id,'title',t.source_definition->>'name','text',i.data::text) AS data FROM relay_event_deliveries d JOIN relay_event_inbox i ON i.workspace=d.workspace AND i.source=d.source AND i.id=d.event_id JOIN relay_event_triggers t ON t.workspace=d.workspace AND t.id=d.trigger_id WHERE d.workspace=$1 AND (d.reply=$2{handoff}) AND i.data IS NOT NULL AND NOT d.revoked"
            );
            json_rows(
                &tx,
                &sql,
                vec![job.workspace.clone().into(), job.reply.clone().into()],
            )
            .await?
        } else {
            vec![]
        };
        tx.commit().await?;
        Ok(sources)
    }
}

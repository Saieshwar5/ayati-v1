use super::*;
use crate::identity::chat::store::NewMessage;
use serde_json::json;
pub(super) async fn cancel_pending(db: &impl ConnectionTrait, w: &str, id: &str) -> Result<()> {
    execute(db,"UPDATE relay_event_deliveries SET state='cancelled',detail='Event trigger paused before this delivery was admitted.' WHERE workspace=$1 AND trigger_id=$2 AND state='pending'",vec![w.into(),id.into()]).await?;
    for row in rows(db,"UPDATE relay_chat_jobs j SET state='finished',lease_until=0,finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE j.workspace=$1 AND j.state='queued' AND j.run_cycle=0 AND j.attempts=0 AND EXISTS(SELECT 1 FROM relay_event_deliveries d WHERE d.workspace=j.workspace AND d.reply=j.reply AND d.trigger_id=$2) RETURNING j.reply",vec![w.into(),id.into()]).await? {
        execute(db,"UPDATE relay_chat_messages SET state='cancelled',content='Event request cancelled because its trigger was paused.' WHERE workspace=$1 AND id=$2 AND state='queued'",vec![w.into(),row.try_get::<String>("","reply")?.into()]).await?;
    }
    Ok(())
}
pub(super) async fn invalidate_inaccessible(db: &DatabaseTransaction, w: &str) -> Result<()> {
    if !super::event_access::exists(db).await? {
        return Ok(());
    }
    for data in json_rows(
        db,
        "SELECT to_jsonb(t) AS data FROM relay_event_triggers t WHERE workspace=$1",
        vec![w.into()],
    )
    .await?
    {
        let expected = json!({"connection_revision":data["connection_revision"],"read_revision":data["read_revision"],"delegation_revision":data["delegation_revision"]});
        let current = super::event_access::current(db, w, &data).await;
        if let Err(ref e) = current
            && e.is::<sea_orm::DbErr>()
        {
            return Err(current.unwrap_err());
        }
        let read = current.ok().as_ref() == Some(&expected);
        let start_access = super::automation_access::authority(
            db,
            &super::event_access::principal(&data)?,
            w,
            data["agent"].as_str().unwrap(),
            data["chat"].as_str().unwrap(),
        )
        .await;
        if start_access
            .as_ref()
            .is_err_and(|e| e.is::<sea_orm::DbErr>())
        {
            return Err(start_access.unwrap_err());
        }
        let starts = start_access.is_ok();
        if read && starts {
            continue;
        }
        if data["state"] != "blocked" {
            execute(db,"UPDATE relay_event_triggers SET state='blocked',revision=revision+1,detail='Current source or destination authority changed. Review before resuming.' WHERE workspace=$1 AND id=$2",vec![w.into(),data["id"].as_str().unwrap().into()]).await?;
            execute(db,"UPDATE relay_event_deliveries SET state='cancelled',detail='Future intake blocked because destination or request authority changed.' WHERE workspace=$1 AND trigger_id=$2 AND state='pending'",vec![w.into(),data["id"].as_str().unwrap().into()]).await?;
        }
        if !read {
            let count=execute(db,"UPDATE relay_event_deliveries SET revoked=true,state=CASE WHEN state='pending' THEN 'cancelled' ELSE state END,detail='Source access changed; this delivery cannot be revived.' WHERE workspace=$1 AND trigger_id=$2 AND NOT revoked",vec![w.into(),data["id"].as_str().unwrap().into()]).await?;
            if count > 0 {
                query(
                    db,
                    "SELECT relay_event_access_changed($1,$2,$3)",
                    vec![
                        w.into(),
                        data["source"].as_str().unwrap().into(),
                        data["chat"].as_str().unwrap().into(),
                    ],
                )
                .await?;
            }
        }
    }
    Ok(())
}
impl PgStore {
    pub(crate) async fn dispatch_events(&self) -> Result<usize> {
        if !self.pooled {
            return Ok(0);
        }
        let candidates=rows(&self.db,"SELECT d.workspace,d.id,t.agent FROM relay_event_deliveries d JOIN relay_event_triggers t ON t.workspace=d.workspace AND t.id=d.trigger_id JOIN relay_event_inbox i ON i.workspace=d.workspace AND i.source=d.source AND i.id=d.event_id WHERE d.state='pending' AND NOT d.revoked AND NOT EXISTS(SELECT 1 FROM relay_event_deliveries earlier JOIN relay_event_inbox ei ON ei.workspace=earlier.workspace AND ei.source=earlier.source AND ei.id=earlier.event_id WHERE earlier.workspace=d.workspace AND earlier.trigger_id=d.trigger_id AND earlier.state='pending' AND NOT earlier.revoked AND ei.sequence<i.sequence) AND NOT EXISTS(SELECT 1 FROM relay_event_deliveries active JOIN relay_chat_jobs j ON j.workspace=active.workspace AND j.reply=active.reply WHERE active.workspace=d.workspace AND active.trigger_id=d.trigger_id AND j.state<>'finished') ORDER BY d.checked_ms,i.sequence,d.id LIMIT 20",vec![]).await?;
        let mut count = 0;
        for row in candidates {
            count += usize::from(
                self.dispatch_event(
                    &row.try_get::<String>("", "workspace")?,
                    &row.try_get::<String>("", "agent")?,
                    &row.try_get::<String>("", "id")?,
                )
                .await?,
            );
        }
        // Completed or unmatched receipt IDs/hashes remain; unnecessary private
        // payloads are removed without allowing the same delivery to run again.
        execute(&self.db,"UPDATE relay_event_inbox i SET data=NULL WHERE (workspace,source,id) IN (SELECT i.workspace,i.source,i.id FROM relay_event_inbox i WHERE i.data IS NOT NULL AND NOT EXISTS(SELECT 1 FROM relay_event_deliveries d LEFT JOIN relay_chat_jobs j ON j.workspace=d.workspace AND j.reply=d.reply WHERE d.workspace=i.workspace AND d.source=i.source AND d.event_id=i.id AND (d.state='pending' OR (d.state='accepted' AND j.state<>'finished'))) LIMIT 100)",vec![]).await?;
        Ok(count)
    }
    async fn dispatch_event(&self, w: &str, agent: &str, id: &str) -> Result<bool> {
        let tx = self.workspace_tx(w).await?;
        invalidate_inaccessible(&tx, w).await?;
        let row=query(&tx,"SELECT to_jsonb(t) AS data,d.authority,d.event_id,d.source FROM relay_event_deliveries d JOIN relay_event_triggers t ON t.workspace=d.workspace AND t.id=d.trigger_id WHERE d.workspace=$1 AND d.id=$2 AND d.state='pending' AND NOT d.revoked AND t.state='enabled'",vec![w.into(),id.into()]).await?;
        let Some(row) = row else {
            tx.commit().await?;
            return Ok(false);
        };
        let data: Value = row.try_get("", "data")?;
        ensure!(data["agent"] == agent, "Event target changed");
        let authority: Value = row.try_get("", "authority")?;
        ensure!(
            super::event_access::current(&tx, w, &data).await? == authority,
            "Event authority changed"
        );
        if query(&tx,"SELECT j.reply FROM relay_event_deliveries d JOIN relay_chat_jobs j ON j.workspace=d.workspace AND j.reply=d.reply WHERE d.workspace=$1 AND d.trigger_id=$2 AND j.state<>'finished' LIMIT 1",vec![w.into(),data["id"].as_str().unwrap().into()]).await?.is_some(){tx.commit().await?;return Ok(false)}
        execute(&tx,"UPDATE relay_event_deliveries SET checked_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE workspace=$1 AND id=$2",vec![w.into(),id.into()]).await?;
        let input = NewMessage {
            id: uuid::Uuid::new_v4().to_string(),
            agent: Some(agent.into()),
            thread: None,
            attachments: vec![],
            content: format!(
                "Connected event work · {}\n{}",
                data["input"]["name"].as_str().unwrap(),
                data["input"]["instructions"].as_str().unwrap()
            ),
        };
        let nested = tx.begin().await?;
        let job = match self
            .background_in(
                &nested,
                &super::event_access::principal(&data)?,
                w,
                data["chat"].as_str().unwrap(),
                &input,
            )
            .await
        {
            Ok(job) => {
                nested.commit().await?;
                job.context("Event request was not accepted")?
            }
            Err(error) => {
                nested.rollback().await?;
                if error.is::<crate::identity::chat::store::QueueFull>()
                    || error.is::<crate::identity::chat::store::BackgroundQueueFull>()
                {
                    tx.commit().await?;
                    return Ok(false);
                }
                if error.is::<sea_orm::DbErr>() {
                    return Err(error);
                }
                execute(&tx,"UPDATE relay_event_deliveries SET state='cancelled',detail='Saved event task references are no longer accepted.' WHERE workspace=$1 AND id=$2",vec![w.into(),id.into()]).await?;
                tx.commit().await?;
                return Ok(false);
            }
        };
        execute(&tx,"UPDATE relay_event_deliveries SET state='accepted',message=$3,reply=$4 WHERE workspace=$1 AND id=$2 AND state='pending'",vec![w.into(),id.into(),input.id.into(),job.reply.clone().into()]).await?;
        audit(
            &tx,
            data["principal"].as_str().unwrap(),
            Some(w),
            "agent_event_delivery_admitted",
            &format!("{id}/{}", job.reply),
        )
        .await?;
        tx.commit().await?;
        Ok(true)
    }
}

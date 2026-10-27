//! Atomic schedule occurrence admission. Execution remains in the ordinary worker pool.
use super::automation_access::{authority, principal, record};
use super::*;
use crate::identity::{automations::model::due, chat::store::NewMessage};
pub(super) async fn cancel_unstarted(tx: &DatabaseTransaction, w: &str, id: &str) -> Result<()> {
    let replies=rows(tx,"UPDATE relay_chat_jobs j SET state='finished',lease_until=0,finished_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE j.workspace=$1 AND j.state='queued' AND j.run_cycle=0 AND j.attempts=0 AND EXISTS(SELECT 1 FROM relay_schedule_occurrences o WHERE o.workspace=j.workspace AND o.schedule=$2 AND o.reply=j.reply) RETURNING j.reply",vec![w.into(),id.into()]).await?;
    for row in replies {
        execute(tx,"UPDATE relay_chat_messages SET state='cancelled',content='Scheduled request cancelled because its routine was paused.' WHERE workspace=$1 AND id=$2 AND state='queued'",vec![w.into(),row.try_get::<String>("","reply")?.into()]).await?;
    }
    Ok(())
}
impl PgStore {
    pub(crate) async fn dispatch_schedules(&self) -> Result<usize> {
        let now_ms: i64 = query(
            &self.db,
            "SELECT (extract(epoch FROM clock_timestamp())*1000)::bigint AS now",
            vec![],
        )
        .await?
        .unwrap()
        .try_get("", "now")?;
        self.dispatch_schedules_at(now_ms).await
    }
    pub(crate) async fn dispatch_schedules_at(&self, now_ms: i64) -> Result<usize> {
        if !self.pooled {
            return Ok(0);
        }
        let candidates=rows(&self.db,"SELECT s.workspace,s.agent,s.id FROM relay_agent_schedules s WHERE s.state='enabled' AND s.next_ms<=$1 AND NOT EXISTS(SELECT 1 FROM relay_schedule_occurrences o JOIN relay_chat_jobs j ON j.workspace=o.workspace AND j.reply=o.reply WHERE o.workspace=s.workspace AND o.schedule=s.id AND j.state<>'finished') ORDER BY s.checked_ms,s.next_ms,s.created_ms,s.id LIMIT 20",vec![now_ms.into()]).await?;
        let mut admitted = 0;
        for row in candidates {
            admitted += usize::from(
                self.dispatch_schedule(
                    &row.try_get::<String>("", "workspace")?,
                    &row.try_get::<String>("", "agent")?,
                    &row.try_get::<String>("", "id")?,
                    now_ms,
                )
                .await?,
            );
        }
        Ok(admitted)
    }
    async fn dispatch_schedule(&self, w: &str, agent: &str, id: &str, now_ms: i64) -> Result<bool> {
        let tx = self.workspace_tx(w).await?;
        let data = record(&tx, w, agent, id).await?;
        if data["state"] != "enabled" || data["next_ms"].as_i64().is_none_or(|next| next > now_ms) {
            tx.commit().await?;
            return Ok(false);
        }
        execute(
            &tx,
            "UPDATE relay_agent_schedules SET checked_ms=$3 WHERE workspace=$1 AND id=$2",
            vec![w.into(), id.into(), now_ms.into()],
        )
        .await?;
        let person = principal(&data)?;
        if !super::home_update_runs::schedule_ready(&tx, w, id).await? {
            tx.commit().await?;
            return Ok(false);
        }
        if let Err(error) = if super::home_update_access::panel_schedule(&tx, w, id).await? {
            Ok(())
        } else {
            authority(&tx, &person, w, agent, data["chat"].as_str().unwrap()).await
        } {
            if error.is::<sea_orm::DbErr>() {
                return Err(error);
            }
            execute(&tx,"UPDATE relay_agent_schedules SET state='blocked',revision=revision+1,detail='Routine access or destination changed. The original person must review and resume.' WHERE workspace=$1 AND id=$2",vec![w.into(),id.into()]).await?;
            tx.commit().await?;
            return Ok(false);
        }
        if query(&tx,"SELECT o.reply FROM relay_schedule_occurrences o JOIN relay_chat_jobs j ON j.workspace=o.workspace AND j.reply=o.reply WHERE o.workspace=$1 AND o.schedule=$2 AND j.state<>'finished' LIMIT 1",vec![w.into(),id.into()]).await?.is_some() {
            tx.commit().await?;return Ok(false);
        }
        let first = data["first_ms"].as_i64().context("Missing first run")?;
        let (at, next) = due(first, data["interval_seconds"].as_i64(), now_ms);
        ensure!(
            at >= data["next_ms"].as_i64().unwrap(),
            "Routine timing moved backwards"
        );
        let input = NewMessage {
            id: uuid::Uuid::new_v4().to_string(),
            content: format!(
                "Scheduled work · {}\n{}",
                data["name"].as_str().unwrap(),
                data["content"].as_str().unwrap()
            ),
            agent: Some(agent.into()),
            thread: None,
            attachments: vec![],
        };
        let intake = tx.begin().await?;
        let job = match self
            .background_in(&intake, &person, w, data["chat"].as_str().unwrap(), &input)
            .await
        {
            Ok(job) => {
                intake.commit().await?;
                job.context("New scheduled request was not accepted")?
            }
            Err(error) => {
                intake.rollback().await?;
                if error.is::<crate::identity::chat::store::QueueFull>()
                    || error.is::<crate::identity::chat::store::BackgroundQueueFull>()
                {
                    execute(&tx,"UPDATE relay_agent_schedules SET detail='Waiting for background queue capacity. The due occurrence remains saved.' WHERE workspace=$1 AND id=$2",vec![w.into(),id.into()]).await?;
                    tx.commit().await?;
                    return Ok(false);
                }
                if error.is::<sea_orm::DbErr>() {
                    return Err(error);
                }
                execute(&tx,"UPDATE relay_agent_schedules SET state='blocked',revision=revision+1,detail='Saved task references are no longer accepted in the destination. Review the instructions and current access.' WHERE workspace=$1 AND id=$2",vec![w.into(),id.into()]).await?;
                tx.commit().await?;
                return Ok(false);
            }
        };
        execute(&tx,"INSERT INTO relay_schedule_occurrences(workspace,schedule,due_ms,message,reply) VALUES($1,$2,$3,$4,$5)",vec![w.into(),id.into(),at.into(),input.id.into(),job.reply.clone().into()]).await?;
        execute(&tx,"UPDATE relay_agent_schedules SET next_ms=$3,state=CASE WHEN $3::bigint IS NULL THEN 'completed' ELSE 'enabled' END,revision=revision+CASE WHEN $3::bigint IS NULL THEN 1 ELSE 0 END,detail='' WHERE workspace=$1 AND id=$2",vec![w.into(),id.into(),next.into()]).await?;
        super::home_update_runs::bind_schedule(&tx, w, id, &job).await?;
        audit(
            &tx,
            &person.id,
            Some(w),
            "agent_schedule_occurrence",
            &format!("{id}/{at}/{}", job.reply),
        )
        .await?;
        tx.commit().await?;
        Ok(true)
    }
}

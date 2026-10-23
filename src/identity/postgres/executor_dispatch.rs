//! Reserve only a connected, compatible ready slot. PG remains the sole owner ledger.
use super::{
    worker_control::{CLOCK, require},
    *,
};
use crate::identity::{executor::model::Profile, worker_control::model::Scope};

#[derive(Clone, Copy)]
pub(crate) struct PoolLimits {
    pub capacity: usize,
    pub workspace: usize,
    pub model: usize,
    pub lease: u64,
    pub chat_reserve: usize,
}
impl PgStore {
    pub(crate) async fn reserve_ready_slot(
        &self,
        executor: &str,
        incarnation: &str,
        limits: PoolLimits,
        connected: &[(String, String)],
    ) -> Result<Option<Scope>> {
        ensure!(
            (1..=1024).contains(&limits.capacity)
                && (1..=1024).contains(&limits.workspace)
                && (1..=1024).contains(&limits.model)
                && (6..=300).contains(&limits.lease),
            "Invalid pool limits"
        );
        ensure!(
            limits.chat_reserve < limits.capacity.min(limits.workspace).min(limits.model),
            "Chat reserve must be smaller than every admission limit"
        );
        let tx = self.db.begin().await?;
        super::assignments::admission_gate(&tx).await?;
        self.recover_assignments_in(&tx).await?;
        let count:i64=query(&tx,&format!("SELECT count(*) AS n FROM relay_chat_jobs WHERE dispatch IN ('assigned','pooled') AND state='running' AND lease_until>{CLOCK}"),vec![]).await?.context("Active count unavailable")?.try_get("","n")?;
        if count >= limits.capacity as i64 {
            return Ok(None);
        }
        let slots=rows(&tx,&format!("SELECT s.* FROM relay_executor_slots s JOIN relay_executors e ON e.id=s.executor AND e.incarnation=s.incarnation
            WHERE s.executor=$1 AND s.incarnation=$2 AND s.state='ready' AND s.preparation_version=1 AND e.state='serving' AND e.expires>{CLOCK} ORDER BY s.slot FOR UPDATE OF s SKIP LOCKED"),vec![executor.into(),incarnation.into()]).await?;
        for slot in slots {
            let profile = Profile {
                provider: slot.try_get("", "provider")?,
                model: slot.try_get("", "model")?,
            };
            let peers = serde_json::json!(connected.iter().map(|(executor,incarnation)|serde_json::json!({"executor":executor,"incarnation":incarnation})).collect::<Vec<_>>());
            let candidate = query(
                &tx,
                include_str!("pool_candidate.sql"),
                vec![
                    profile.model.clone().into(),
                    profile.provider.clone().into(),
                    (limits.workspace as i64).into(),
                    (limits.model as i64).into(),
                    (limits.capacity as i64).into(),
                    count.into(),
                    (limits.chat_reserve as i64).into(),
                    peers.into(),
                ],
            )
            .await?;
            let Some(candidate) = candidate else {
                continue;
            };
            let reply: String = candidate.try_get("", "reply")?;
            let class: String = candidate.try_get("", "work_class")?;
            let id = uuid::Uuid::new_v4().to_string();
            execute(&tx,&format!("UPDATE relay_chat_jobs SET state='running',claim=$2,attempts=attempts+1,lease_until={CLOCK}+15 WHERE reply=$1"),vec![reply.clone().into(),id.clone().into()]).await?;
            execute(&tx,"INSERT INTO relay_worker_assignments(id,workspace,reply,generation,provider,state,lease_seconds) VALUES($1,$2,$3,$4,'local','launching',$5)",
                vec![id.clone().into(),candidate.try_get::<String>("","workspace")?.into(),reply.into(),(candidate.try_get::<i32>("","attempts")?+1).into(),(limits.lease as i32).into()]).await?;
            let slot_id: String = slot.try_get("", "slot")?;
            let generation: i64 = slot.try_get("", "generation")?;
            execute(&tx,&format!("INSERT INTO relay_assignment_credentials(assignment,executor,incarnation,slot,slot_generation,request,token_hash,expires)
                VALUES($1,$2,$3,$4,$5,$1,$6,{CLOCK}+300)"),vec![id.clone().into(),executor.into(),incarnation.into(),slot_id.clone().into(),generation.into(),slot.try_get::<String>("","token_hash")?.into()]).await?;
            let hash: String = slot.try_get("", "token_hash")?;
            let record = super::worker_control::controlled(&tx, &id, &hash).await?;
            execute(&tx,"UPDATE relay_executor_slots SET state='offered',assignment=$4 WHERE executor=$1 AND incarnation=$2 AND slot=$3",vec![executor.into(),incarnation.into(),slot_id.into(),id.clone().into()]).await?;
            execute(
                &tx,
                "UPDATE relay_worker_assignments SET resource_id=$2 WHERE id=$1",
                vec![id.into(), record.scope.resource().into()],
            )
            .await?;
            execute(&tx,"INSERT INTO relay_pool_turns(provider,model,interactive_streak) VALUES($1,$2,CASE WHEN $3='interactive' THEN 1 ELSE 0 END)
                ON CONFLICT(provider,model) DO UPDATE SET interactive_streak=CASE WHEN $3='interactive' THEN LEAST(relay_pool_turns.interactive_streak+1,3) ELSE 0 END",
                vec![profile.provider.into(),profile.model.into(),class.into()]).await?;
            tx.commit().await?;
            return Ok(Some(record.scope));
        }
        tx.commit().await?;
        Ok(None)
    }
    pub(crate) async fn pending_offers(
        &self,
        executor: &str,
        incarnation: &str,
    ) -> Result<Vec<Scope>> {
        let records=rows(&self.db,&format!("SELECT c.assignment,c.token_hash FROM relay_assignment_credentials c
            JOIN relay_executor_slots s ON s.executor=c.executor AND s.incarnation=c.incarnation AND s.slot=c.slot AND s.generation=c.slot_generation AND s.assignment=c.assignment
            JOIN relay_chat_jobs j ON j.claim=c.assignment WHERE c.executor=$1 AND c.incarnation=$2 AND s.state='offered'
            AND j.state='running' AND j.lease_until>{CLOCK}"),vec![executor.into(),incarnation.into()]).await?;
        let mut scopes = Vec::new();
        for row in records {
            if let Ok(record) = super::worker_control::controlled(
                &self.db,
                &row.try_get::<String>("", "assignment")?,
                &row.try_get::<String>("", "token_hash")?,
            )
            .await
            {
                scopes.push(record.scope);
            }
        }
        Ok(scopes)
    }
    pub(super) async fn acknowledge_slot(
        &self,
        tx: &DatabaseTransaction,
        scope: &Scope,
    ) -> Result<()> {
        if scope.slot_generation == 0 {
            return Ok(());
        }
        require(execute(tx,"UPDATE relay_executor_slots SET state='running' WHERE executor=$1 AND incarnation=$2 AND slot=$3 AND generation=$4 AND assignment=$5 AND state IN ('offered','running')",
            vec![scope.executor.clone().into(),scope.incarnation.clone().into(),scope.slot.clone().into(),scope.slot_generation.into(),scope.assignment.clone().into()]).await?==1)
    }
}

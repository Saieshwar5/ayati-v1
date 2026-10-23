//! Durable readiness and generation fencing. Reports never grant assignment ownership.
use super::{
    worker_control::{CLOCK, require},
    *,
};
use crate::identity::executor::model::Capacity;

impl PgStore {
    pub(crate) async fn report_executor(
        &self,
        executor: &str,
        incarnation: &str,
        report: &Capacity,
    ) -> Result<()> {
        // Liveness must survive a refused capacity transition. A stopped session
        // may already be replaced locally while its old lease awaits reconciliation;
        // that must not expire the executor's other healthy sessions.
        require(execute(&self.db, &format!("UPDATE relay_executors SET heartbeat={CLOCK},expires={CLOCK}+86400 WHERE id=$1 AND incarnation=$2 AND pooled AND expires>{CLOCK} AND state IN ('serving','draining')"), vec![executor.into(),incarnation.into()]).await? == 1)?;
        ensure!(report.slots.len() <= 16, "Too many executor slots");
        let tx = self.db.begin().await?;
        super::assignments::admission_gate(&tx).await?;
        let state = executor_state(&tx, executor, incarnation).await?;
        require(state == "serving" || state == "draining")?;
        let mut unique = std::collections::HashSet::new();
        for slot in &report.slots {
            crate::identity::worker_control::model::uuid(&slot.slot)?;
            crate::identity::worker_control::model::token(&slot.token)?;
            slot.profile.validate()?;
            ensure!(
                (1..=1_000_000_000).contains(&slot.generation) && unique.insert(&slot.slot),
                "Invalid/duplicate slot generation"
            );
            ensure!(
                ["preparing", "ready", "running"].contains(&slot.state.as_str()),
                "Invalid reported slot state"
            );
            ensure!(
                slot.state != "ready" || slot.pid.is_some_and(|pid| pid > 0),
                "Ready slots need a prepared harness"
            );
            let hash = super::super::provider::digest(&slot.token);
            let old=query(&tx,"SELECT generation,provider,model,token_hash,assignment,state FROM relay_executor_slots WHERE executor=$1 AND incarnation=$2 AND slot=$3 FOR UPDATE",
                vec![executor.into(),incarnation.into(),slot.slot.clone().into()]).await?;
            if let Some(old) = old {
                let generation: i64 = old.try_get("", "generation")?;
                require(slot.generation >= generation)?;
                if slot.generation == generation {
                    require(
                        old.try_get::<String>("", "token_hash")? == hash
                            && old.try_get::<String>("", "provider")? == slot.profile.provider
                            && old.try_get::<String>("", "model")? == slot.profile.model,
                    )?;
                    // An executor may report ready before it receives an already persisted offer.
                    let current: String = old.try_get("", "state")?;
                    if ["offered", "running", "cleaning", "draining", "lost"]
                        .contains(&current.as_str())
                    {
                        continue;
                    }
                    require(slot.state != "running")?;
                } else {
                    require(slot.state != "running")?;
                    if let Some(assignment) = old.try_get::<Option<String>>("", "assignment")? {
                        ensure!(query(&tx,&format!("SELECT reply FROM relay_chat_jobs WHERE claim=$1 AND state='running' AND lease_until>{CLOCK}"),vec![assignment.clone().into()]).await?.is_none(), "Previous generation is still leased");
                        execute(&tx,"UPDATE relay_assignment_credentials SET context=NULL,expires=0 WHERE assignment=$1",vec![assignment.into()]).await?;
                    }
                }
            } else {
                require(slot.state != "running")?;
                let count:i64=query(&tx,"SELECT count(*) AS n FROM relay_executor_slots WHERE executor=$1 AND incarnation=$2",vec![executor.into(),incarnation.into()]).await?.context("Slot count unavailable")?.try_get("","n")?;
                ensure!(count < 16, "Executor slot limit reached");
            }
            require(state == "serving")?;
            execute(&tx,"INSERT INTO relay_executor_slots(executor,incarnation,slot,generation,provider,model,state,token_hash,pid,ready_ms)
                VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,CASE WHEN $7='ready' THEN (extract(epoch FROM clock_timestamp())*1000)::bigint ELSE NULL END)
                ON CONFLICT(executor,incarnation,slot) DO UPDATE SET generation=$4,provider=$5,model=$6,state=$7,token_hash=$8,pid=$9,
                ready_ms=CASE WHEN $7='ready' THEN COALESCE(CASE WHEN relay_executor_slots.generation=$4 THEN relay_executor_slots.ready_ms END,(extract(epoch FROM clock_timestamp())*1000)::bigint) ELSE NULL END,assignment=NULL",
                vec![executor.into(),incarnation.into(),slot.slot.clone().into(),slot.generation.into(),slot.profile.provider.clone().into(),slot.profile.model.clone().into(),slot.state.clone().into(),hash.into(),slot.pid.map(i64::from).into()]).await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn drain_executor(&self, executor: &str, incarnation: &str) -> Result<()> {
        let tx = self.db.begin().await?;
        super::assignments::admission_gate(&tx).await?;
        executor_state(&tx, executor, incarnation).await?;
        execute(&tx,&format!("UPDATE relay_executors SET state='draining',heartbeat={CLOCK} WHERE id=$1 AND incarnation=$2"),vec![executor.into(),incarnation.into()]).await?;
        execute(&tx,"UPDATE relay_executor_slots SET state='draining' WHERE executor=$1 AND incarnation=$2 AND state IN ('preparing','ready')",vec![executor.into(),incarnation.into()]).await?;
        tx.commit().await?;
        Ok(())
    }
    pub(crate) async fn stop_executor(&self, executor: &str, incarnation: &str) -> Result<()> {
        let tx = self.db.begin().await?;
        super::assignments::admission_gate(&tx).await?;
        require(query(&tx,&format!("SELECT j.reply FROM relay_chat_jobs j JOIN relay_assignment_credentials c ON c.assignment=j.claim
            WHERE c.executor=$1 AND c.incarnation=$2 AND j.state='running' AND j.lease_until>{CLOCK} LIMIT 1"),vec![executor.into(),incarnation.into()]).await?.is_none())?;
        execute(
            &tx,
            "UPDATE relay_executors SET state='stopped' WHERE id=$1 AND incarnation=$2 AND pooled",
            vec![executor.into(), incarnation.into()],
        )
        .await?;
        execute(
            &tx,
            "UPDATE relay_executor_slots SET state='draining' WHERE executor=$1 AND incarnation=$2",
            vec![executor.into(), incarnation.into()],
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
    pub(crate) async fn pooled_executor(&self, executor: &str, incarnation: &str) -> Result<()> {
        executor_state(&self.db, executor, incarnation)
            .await
            .map(|_| ())
    }
    pub(crate) async fn expire_executors(&self, seconds: u64) -> Result<()> {
        let tx = self.db.begin().await?;
        super::assignments::admission_gate(&tx).await?;
        self.recover_assignments_in(&tx).await?;
        execute(&tx,&format!("UPDATE relay_executors SET state='lost' WHERE pooled AND state IN ('serving','draining') AND heartbeat<={CLOCK}-$1"),vec![(seconds as i64).into()]).await?;
        execute(&tx,"UPDATE relay_executor_slots s SET state='lost' FROM relay_executors e WHERE e.id=s.executor AND e.incarnation=s.incarnation AND e.state='lost' AND s.state<>'lost'",vec![]).await?;
        tx.commit().await?;
        Ok(())
    }
}
async fn executor_state(
    db: &impl ConnectionTrait,
    executor: &str,
    incarnation: &str,
) -> Result<String> {
    let row=query(db,&format!("SELECT state FROM relay_executors WHERE id=$1 AND incarnation=$2 AND pooled AND expires>{CLOCK} AND state IN ('serving','draining')"),vec![executor.into(),incarnation.into()]).await?.context(super::worker_control::ControlDenied)?;
    Ok(row.try_get("", "state")?)
}

//! What the Cuckoo Connector on a workspace's machine calls. It signs in once
//! with the one-time code and gets a lasting key (kept only as its digest),
//! then asks for work and answers it. Every call says who it is with that key.
use super::{digest, same};
use crate::identity::postgres::*;
use serde_json::json;

/// The key was wrong, or the machine was removed or signed in again (401).
#[derive(Debug)]
pub(crate) struct Refused;
impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("This connector isn't signed in to Cuckoo")
    }
}
impl std::error::Error for Refused {}

impl PgStore {
    /// Signs a connector in with its one-time code. The code works once.
    pub(crate) async fn machine_sign_in(
        &self,
        computer: &str,
        code: &str,
        platform: &str,
    ) -> Result<Value> {
        crate::identity::worker_control::model::uuid(computer).map_err(|_| Refused)?;
        let code = code.trim().to_ascii_uppercase();
        let row = query(&self.db, "SELECT workspace,code,code_expires_ms>=(extract(epoch FROM clock_timestamp())*1000)::bigint AS fresh FROM relay_workspace_computers WHERE id=$1 AND NOT removed AND code IS NOT NULL",
            vec![computer.into()]).await?.ok_or(Refused)?;
        let saved: String = row.try_get("", "code")?;
        if !row.try_get::<bool>("", "fresh")? || !same(&saved, &digest(&code)) {
            return Err(Refused.into());
        }
        let w: String = row.try_get("", "workspace")?;
        let key = format!("ckc_{}", crate::identity::provider::random_token());
        let platform: String = platform
            .chars()
            .filter(|c| !c.is_control())
            .take(100)
            .collect();
        let changed = execute(&self.db, "UPDATE relay_workspace_computers SET device=$3,code=NULL,code_expires_ms=NULL,platform=$4,last_seen_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint,revision=revision+1
            WHERE workspace=$1 AND id=$2 AND code=$5", vec![w.into(), computer.into(), digest(&key).into(), platform.into(), saved.into()]).await?;
        ensure!(changed == 1, Refused);
        Ok(json!({"key":key}))
    }

    /// The workspace of a signed-in connector; also marks it seen.
    async fn machine(&self, computer: &str, key: &str) -> Result<String> {
        crate::identity::worker_control::model::uuid(computer).map_err(|_| Refused)?;
        let row = query(&self.db, "SELECT workspace,device FROM relay_workspace_computers WHERE id=$1 AND NOT removed AND device IS NOT NULL",
            vec![computer.into()]).await?.ok_or(Refused)?;
        let device: String = row.try_get("", "device")?;
        if !same(&device, &digest(key)) {
            return Err(Refused.into());
        }
        let w: String = row.try_get("", "workspace")?;
        execute(&self.db, "UPDATE relay_workspace_computers SET last_seen_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint WHERE workspace=$1 AND id=$2",
            vec![w.clone().into(), computer.into()]).await?;
        Ok(w)
    }

    /// The next piece of work for this machine, waiting up to `wait` for one.
    pub(crate) async fn machine_next(
        &self,
        computer: &str,
        key: &str,
        wait: std::time::Duration,
    ) -> Result<Value> {
        let w = self.machine(computer, key).await?;
        let started = std::time::Instant::now();
        let mut marked = started;
        loop {
            let taken = query(&self.db, "UPDATE relay_workspace_computer_calls SET state='taken',taken_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint
                WHERE id=(SELECT id FROM relay_workspace_computer_calls WHERE workspace=$1 AND computer=$2 AND state='queued' ORDER BY created_ms,id LIMIT 1 FOR UPDATE SKIP LOCKED)
                RETURNING jsonb_build_object('id',id,'kind',kind,'agent',agent) || request AS data", vec![w.clone().into(), computer.into()]).await?;
            if let Some(row) = taken {
                return Ok(json!({"call":row.try_get::<Value>("", "data")?}));
            }
            if started.elapsed() >= wait {
                return Ok(json!({"call":null}));
            }
            tokio::time::sleep(std::time::Duration::from_millis(250)).await;
            // Keep the machine marked online while it waits, and stop waiting
            // as soon as it is removed or signed in elsewhere.
            if marked.elapsed() >= std::time::Duration::from_secs(10) {
                self.machine(computer, key).await?;
                marked = std::time::Instant::now();
            }
        }
    }

    /// The answer to one piece of work. Only work this machine took is answered.
    pub(crate) async fn machine_done(
        &self,
        computer: &str,
        key: &str,
        call: &str,
        result: &Value,
    ) -> Result<Value> {
        let w = self.machine(computer, key).await?;
        ensure!(
            serde_json::to_vec(result)?.len() <= 1_400_000,
            "The answer is too large"
        );
        let changed = execute(&self.db, "UPDATE relay_workspace_computer_calls SET state='done',result=$4,done_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint
            WHERE workspace=$1 AND computer=$2 AND id=$3 AND state='taken'", vec![w.into(), computer.into(), call.into(), result.clone().into()]).await?;
        Ok(json!({"saved":changed == 1}))
    }
}

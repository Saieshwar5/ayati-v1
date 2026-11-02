//! Panel updates the controller runs itself (Phase 3 HP3): it performs the panel's
//! reviewed reads through the ordinary action path, fills the numbers from the saved
//! bindings and publishes, with no model call. Anything that doesn't fit hands the
//! same update back to the agent.
use super::super::super::*;
use crate::identity::chat::model::Job;
use crate::panels::bindings::Binding;

/// What a controller-run update needs: the reads, the bindings, the section as last
/// published (or as reviewed) and the panel's id.
pub(crate) struct Plan {
    pub scopes: Vec<Value>,
    pub bindings: Vec<Binding>,
    pub section: Value,
}

impl PgStore {
    /// Take the updates claimed for the controller; each is taken once.
    pub(crate) async fn direct_pick(&self) -> Result<Vec<Job>> {
        if !super::available(&self.db).await? {
            return Ok(vec![]);
        }
        let taken = rows(&self.db, "WITH ready AS (SELECT reply FROM relay_chat_jobs WHERE state='running' AND claim LIKE 'direct-%' AND lease_until>floor(extract(epoch FROM clock_timestamp()))::bigint ORDER BY started_ms LIMIT 8 FOR UPDATE SKIP LOCKED)
            UPDATE relay_chat_jobs j SET claim='directrun-'||substr(j.claim,8) FROM ready WHERE j.reply=ready.reply RETURNING j.payload,j.claim", vec![]).await?;
        taken
            .into_iter()
            .map(|row| {
                let mut job: Job = serde_json::from_value(row.try_get("", "payload")?)?;
                job.claim = Some(row.try_get("", "claim")?);
                Ok(job)
            })
            .collect()
    }

    pub(crate) async fn direct_plan(&self, job: &Job) -> Result<Plan> {
        let tx = self.read_tx(&job.workspace).await?;
        super::super::super::control_actions::authorize(&tx, job, true).await?;
        let data = super::super::super::home_update_access::active(&tx, job)
            .await?
            .context("Not a panel update")?;
        let bindings: Vec<Binding> = serde_json::from_value(data["policy"]["bindings"].clone())
            .context("No saved bindings")?;
        let latest = query(&tx, "SELECT definition FROM relay_home_snapshots WHERE workspace=$1 AND panel=$2 AND panel_revision=$3 ORDER BY generation DESC LIMIT 1",
            vec![job.workspace.clone().into(), data["panel"].as_str().unwrap_or_default().into(), data["panel_revision"].as_i64().unwrap_or(0).into()]).await?;
        let definition = match latest {
            Some(row) => row.try_get::<Value>("", "definition")?,
            None => data["definition"].clone(),
        };
        tx.commit().await?;
        Ok(Plan {
            scopes: data["policy"]["scopes"]
                .as_array()
                .cloned()
                .unwrap_or_default(),
            bindings,
            section: definition["sections"][0].clone(),
        })
    }

    pub(crate) async fn direct_results(&self, job: &Job, scopes: &[Value]) -> Result<Vec<Value>> {
        super::read_results(&self.db, job, scopes).await
    }

    /// Published without the agent.
    pub(crate) async fn direct_done(&self, job: &Job) -> Result<()> {
        execute(
            &self.db,
            "UPDATE relay_home_update_runs SET direct=true WHERE workspace=$1 AND reply=$2",
            vec![job.workspace.clone().into(), job.reply.clone().into()],
        )
        .await?;
        Ok(())
    }

    /// Hand the update to the agent, saying why; it runs like any other update.
    pub(crate) async fn direct_release(&self, job: &Job, why: &str) -> Result<bool> {
        let tx = self.workspace_tx(&job.workspace).await?;
        let why: String = why.chars().take(300).collect();
        execute(
            &tx,
            "UPDATE relay_home_update_runs SET fallback=$3 WHERE workspace=$1 AND reply=$2",
            vec![
                job.workspace.clone().into(),
                job.reply.clone().into(),
                why.into(),
            ],
        )
        .await?;
        let released = execute(&tx, "UPDATE relay_chat_jobs SET state='queued',claim=NULL,lease_until=0 WHERE workspace=$1 AND reply=$2 AND claim=$3 AND state='running'",
            vec![job.workspace.clone().into(), job.reply.clone().into(), job.claim.clone().into()]).await? == 1;
        tx.commit().await?;
        Ok(released)
    }
}

#[cfg(test)]
#[path = "home_direct_tests.rs"]
mod tests;

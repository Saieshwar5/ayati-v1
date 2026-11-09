//! Drift check runs and results (schema 110), and each connector provider's
//! health for Cuckoo Control: calls, errors and estimated cost this month, its
//! circuit breaker and the last drift check.
use super::super::*;
use crate::identity::connectors::drift::{Finding, nightly_slot};
use crate::identity::control::model::Operation;
use serde_json::json;

/// A run's lease, renewed after each account it checks.
const LEASE_MS: i64 = 600_000;
/// Runs (and their results) older than this are removed when a run finishes.
const KEEP_MS: i64 = 30 * 86_400_000;
const NOW: &str = "(extract(epoch FROM clock_timestamp())*1000)::bigint";

/// A connected account the drift check reads. Provider ids, never tokens.
pub(crate) struct DriftAccount {
    pub workspace: String,
    pub workspace_name: String,
    pub connection: String,
    pub server: String,
    pub name: String,
    pub provider: String,
    pub binding: Option<Value>,
    pub fingerprint: String,
    pub operations: Vec<Operation>,
}

async fn begin(db: &impl ConnectionTrait, via: &str) -> Result<String> {
    let id = uuid::Uuid::new_v4().to_string();
    execute(db, &format!("INSERT INTO relay_connector_drift_runs(id,via,started_ms,lease_until_ms) VALUES($1,$2,{NOW},{NOW}+{LEASE_MS})"),
        vec![id.clone().into(), via.into()]).await?;
    Ok(id)
}

impl PgStore {
    /// Starts tonight's run when it is due and no controller holds a live one.
    pub(crate) async fn connector_drift_claim(&self) -> Result<Option<String>> {
        let tx = self.db.begin().await?;
        execute(
            &tx,
            "SELECT pg_advisory_xact_lock(hashtext('relay_connector_drift_runs'))",
            vec![],
        )
        .await?;
        let row = query(&tx, &format!("SELECT {NOW} AS now,
            EXISTS(SELECT 1 FROM relay_connector_drift_runs WHERE finished_ms IS NULL AND lease_until_ms>{NOW}) AS busy,
            (SELECT max(started_ms) FROM relay_connector_drift_runs WHERE finished_ms IS NOT NULL) AS last"), vec![])
            .await?
            .context("Drift runs")?;
        let now: i64 = row.try_get("", "now")?;
        let last: Option<i64> = row.try_get("", "last")?;
        if row.try_get::<bool>("", "busy")? || last.is_some_and(|l| l >= nightly_slot(now)) {
            return Ok(None);
        }
        let id = begin(&tx, "nightly").await?;
        tx.commit().await?;
        Ok(Some(id))
    }

    /// A run for `ayati admin connectors verify`, whatever the nightly one does.
    pub(crate) async fn connector_drift_begin(&self) -> Result<String> {
        begin(&self.db, "cli").await
    }

    /// Every connected account in use, and how many were skipped because they
    /// need reconnecting, their provider is switched off or the workspace closed.
    pub(crate) async fn connector_drift_accounts(&self) -> Result<(Vec<DriftAccount>, usize)> {
        let found = rows(&self.db, "SELECT c.workspace,w.name AS workspace_name,c.id,c.server,COALESCE(c.definition->>'name',c.server) AS name,
              c.provider,c.binding,c.fingerprint,c.definition->'operations' AS operations,
              (w.closed_ms IS NOT NULL OR EXISTS(SELECT 1 FROM relay_connector_reconnects n WHERE n.workspace=c.workspace AND n.connection=c.id)
               OR EXISTS(SELECT 1 FROM relay_connector_providers p WHERE p.provider=c.provider AND NOT p.enabled)) AS skip
            FROM relay_control_connections c JOIN relay_workspaces w ON w.id=c.workspace
            JOIN relay_control_systems s ON s.workspace=c.workspace AND s.server=c.server AND s.approved
            WHERE c.enabled ORDER BY w.name,c.workspace,c.server", vec![]).await?;
        let (mut accounts, mut skipped) = (vec![], 0);
        for row in found {
            if row.try_get::<bool>("", "skip")? {
                skipped += 1;
                continue;
            }
            let operations: Value = row.try_get("", "operations")?;
            accounts.push(DriftAccount {
                workspace: row.try_get("", "workspace")?,
                workspace_name: row.try_get("", "workspace_name")?,
                connection: row.try_get("", "id")?,
                server: row.try_get("", "server")?,
                name: row.try_get("", "name")?,
                provider: row.try_get("", "provider")?,
                binding: row.try_get("", "binding")?,
                fingerprint: row.try_get("", "fingerprint")?,
                operations: serde_json::from_value(operations).unwrap_or_default(),
            });
        }
        Ok((accounts, skipped))
    }

    /// One account's results; renews the run's lease.
    pub(crate) async fn connector_drift_save(
        &self,
        run: &str,
        account: &DriftAccount,
        findings: &[(&Operation, Finding)],
    ) -> Result<()> {
        let tx = self.db.begin().await?;
        for (operation, finding) in findings {
            execute(&tx, "INSERT INTO relay_connector_drift(run,workspace,connection,fingerprint,provider,operation,status,fields,version) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT DO NOTHING",
                vec![run.into(), account.workspace.clone().into(), account.connection.clone().into(), account.fingerprint.clone().into(),
                    account.provider.clone().into(), operation.id.clone().into(), finding.status.into(), json!(finding.fields).into(),
                    finding.version.clone().into()]).await?;
        }
        execute(
            &tx,
            &format!(
                "UPDATE relay_connector_drift_runs SET lease_until_ms={NOW}+{LEASE_MS} WHERE id=$1"
            ),
            vec![run.into()],
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub(crate) async fn connector_drift_finish(
        &self,
        run: &str,
        accounts: usize,
        skipped: usize,
    ) -> Result<()> {
        execute(&self.db, &format!("UPDATE relay_connector_drift_runs SET finished_ms={NOW},accounts=$2,skipped=$3 WHERE id=$1"),
            vec![run.into(), (accounts as i32).into(), (skipped as i32).into()]).await?;
        execute(
            &self.db,
            &format!("DELETE FROM relay_connector_drift_runs WHERE started_ms<{NOW}-{KEEP_MS}"),
            vec![],
        )
        .await?;
        Ok(())
    }

    /// Per provider: calls, errors, refusals and estimated cost this month (UTC),
    /// its breaker, and the last finished drift check's counts.
    pub(crate) async fn connector_health(&self, configured: &[&str]) -> Result<Value> {
        let month = super::super::operator::month_start(now() * 1000);
        let calls = json_rows(
            &self.db,
            "SELECT jsonb_build_object('provider',provider,
              'calls',count(*) FILTER (WHERE outcome IS DISTINCT FROM 'refused'),
              'errors',count(*) FILTER (WHERE outcome='uncertain' OR provider_error),
              'refused',count(*) FILTER (WHERE outcome='refused'),
              'usd',COALESCE(sum(usd_estimate),0)) AS data
            FROM relay_connector_calls WHERE started_ms>=$1 GROUP BY provider",
            vec![month.into()],
        )
        .await?;
        let breakers = self.connector_breakers().await?;
        let run = json_rows(&self.db, "SELECT jsonb_build_object('id',id,'at',started_ms,'finished_ms',finished_ms,'via',via,'accounts',accounts,'skipped',skipped) AS data
            FROM relay_connector_drift_runs WHERE finished_ms IS NOT NULL ORDER BY started_ms DESC LIMIT 1", vec![]).await?
            .pop();
        let drift = match &run {
            Some(run) => json_rows(&self.db, "SELECT jsonb_build_object('provider',provider,'ok',count(*) FILTER (WHERE status='ok'),
                  'missing',count(*) FILTER (WHERE status='missing'),'changed',count(*) FILTER (WHERE status='changed'),
                  'new_version',count(*) FILTER (WHERE status='new_version'),'failed',count(*) FILTER (WHERE status='failed')) AS data
                FROM relay_connector_drift WHERE run=$1 GROUP BY provider", vec![run["id"].as_str().unwrap_or_default().into()]).await?,
            None => vec![],
        };
        let mut names: Vec<String> = configured.iter().map(|n| n.to_string()).collect();
        for row in calls.iter().chain(&breakers).chain(&drift) {
            if let Some(name) = row["provider"].as_str()
                && !names.iter().any(|n| n == name)
            {
                names.push(name.to_owned());
            }
        }
        let find =
            |list: &[Value], name: &str| list.iter().find(|r| r["provider"] == name).cloned();
        let health: Vec<Value> = names
            .iter()
            .map(|name| {
                let month = find(&calls, name).unwrap_or_else(|| json!({"calls":0,"errors":0,"refused":0,"usd":0.0}));
                let breaker = find(&breakers, name).unwrap_or_else(|| json!({"state":"closed","failures":0,"until_ms":null,"opened_ms":null}));
                let drift = find(&drift, name).or_else(|| run.as_ref().map(|_| json!({"ok":0,"missing":0,"changed":0,"new_version":0,"failed":0})));
                json!({"provider": name, "month": {"calls": month["calls"], "errors": month["errors"], "refused": month["refused"], "usd": month["usd"]},
                    "breaker": {"state": breaker["state"], "failures": breaker["failures"], "until_ms": breaker["until_ms"], "opened_ms": breaker["opened_ms"]},
                    "drift": drift.map(|d| json!({"ok": d["ok"], "missing": d["missing"], "changed": d["changed"], "new_version": d["new_version"], "failed": d["failed"]}))})
            })
            .collect();
        Ok(json!({"month_start_ms": month, "drift_run": run, "health": health}))
    }
}

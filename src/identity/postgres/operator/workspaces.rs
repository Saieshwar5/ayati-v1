//! Every workspace for operators: counts, spend and status, never contents.
//! Suspension refuses every member (the shared role check) and stops agent work
//! through the same pause that admins use; resuming lifts both.
use super::*;

/// The pause note suspension leaves, so resuming lifts only its own pause.
const SUSPENDED_PAUSE: &str = "Suspended by Cuckoo";
const STOPPED: &str = "Stopped because Cuckoo suspended the workspace.";
const NOT_SENT: &str = "Cuckoo suspended the workspace; no pending operation was dispatched.";

/// Columns shared by the list and the detail view. `$1` is the month start in
/// milliseconds (UTC) and `$2` the start of the 7-day window. Charges and costs are
/// the gateway's per-call figures (schema 88); the budget is the workspace's own
/// monthly model budget (schema 89), set by its admins.
const SUMMARY: &str = "'id',w.id,'name',w.name,
  'created_ms',(SELECT min(a.at)*1000 FROM relay_audit a WHERE a.workspace=w.id AND a.action='workspace_created'),
  'owner',(SELECT u.email FROM relay_members m JOIN relay_users u ON u.id=m.user_id WHERE m.workspace=w.id
     AND m.role IN ('Admin','Owner') ORDER BY m.sequence LIMIT 1),
  'people',(SELECT count(*) FROM relay_members m WHERE m.workspace=w.id),
  'agents',(SELECT count(*) FROM relay_shared_agents a WHERE a.workspace=w.id),
  'running',(SELECT count(*) FROM relay_chat_jobs j WHERE j.workspace=w.id AND j.state<>'finished'),
  'charge',(SELECT COALESCE(sum(u.charge_micro),0) FROM relay_model_usage u WHERE u.workspace=w.id AND u.at_ms>=$1)/1000000.0,
  'cost',(SELECT COALESCE(sum(u.cost_micro),0) FROM relay_model_usage u WHERE u.workspace=w.id AND u.at_ms>=$1)/1000000.0,
  'budget',(SELECT b.monthly_micro/1000000.0 FROM relay_model_budgets b WHERE b.workspace=w.id AND b.agent=''),
  'calls_7d',(SELECT COALESCE(jsonb_agg(c.n ORDER BY c.day),'[]'::jsonb) FROM (SELECT d.day,count(u.id) AS n
     FROM generate_series($2::bigint,$2::bigint+6*86400000,86400000) d(day)
     LEFT JOIN relay_model_usage u ON u.workspace=w.id AND u.at_ms>=d.day AND u.at_ms<d.day+86400000 GROUP BY d.day) c),
  'error_rate_7d',(SELECT CASE WHEN o.calls+o.failed=0 THEN 0 ELSE round(100.0*o.failed/(o.calls+o.failed),1) END FROM (SELECT
     (SELECT count(*) FROM relay_model_usage u WHERE u.workspace=w.id AND u.at_ms>=$2) AS calls,
     (SELECT count(*) FROM relay_model_failures f WHERE f.workspace=w.id AND f.at_ms>=$2) AS failed) o),
  'last_active_ms',(SELECT max(u.at_ms) FROM relay_model_usage u WHERE u.workspace=w.id),
  'status',CASE WHEN to_jsonb(w)->>'closed_ms' IS NOT NULL THEN 'closed'
     WHEN to_jsonb(w)->>'suspended_ms' IS NOT NULL THEN 'suspended'
     WHEN to_jsonb(w)->>'delete_at_ms' IS NOT NULL THEN 'closing'
     WHEN to_jsonb(w)->>'agents_paused_ms' IS NOT NULL THEN 'paused' ELSE 'active' END";

impl PgStore {
    fn summary_args() -> Vec<SqlValue> {
        let (month, day) = month_and_day_start();
        vec![month.into(), (day - 6 * 86_400_000).into()]
    }

    pub(crate) async fn operator_workspaces(&self, search: &str) -> Result<Value> {
        let search = search.trim();
        one_line(search, 120, "search")?;
        let mut args = Self::summary_args();
        args.push(
            format!(
                "%{}%",
                search
                    .replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_")
            )
            .into(),
        );
        let list = json_rows(&self.db, &format!("SELECT jsonb_build_object({SUMMARY}) AS data FROM relay_workspaces w
            WHERE $3='%%' OR w.name ILIKE $3 OR w.id ILIKE $3 OR EXISTS(SELECT 1 FROM relay_members m JOIN relay_users u ON u.id=m.user_id
              WHERE m.workspace=w.id AND u.email ILIKE $3)
            ORDER BY w.sequence DESC LIMIT 500"), args).await?;
        let total: i64 = query(
            &self.db,
            "SELECT count(*) AS n FROM relay_workspaces",
            vec![],
        )
        .await?
        .context("Missing count")?
        .try_get("", "n")?;
        Ok(json!({"workspaces": list, "total": total}))
    }

    pub(crate) async fn operator_workspace(&self, w: &str) -> Result<Value> {
        let mut args = Self::summary_args();
        args.push(w.into());
        let mut data = query(&self.db, &format!("SELECT jsonb_build_object({SUMMARY},
              'admins',(SELECT count(*) FROM relay_members m WHERE m.workspace=w.id AND m.role IN ('Admin','Owner')),
              'connections',(SELECT count(*) FROM relay_control_connections c WHERE c.workspace=w.id AND c.enabled),
              'library_items',(SELECT count(*) FROM relay_library_items l WHERE l.workspace=w.id),
              'library_bytes',(SELECT COALESCE(sum(l.size),0) FROM relay_library_items l WHERE l.workspace=w.id),
              'problem_reports_30d',(SELECT count(*) FROM relay_problem_reports r WHERE r.workspace=w.id
                 AND r.created_ms>=(extract(epoch FROM now())*1000)::bigint-30::bigint*86400000),
              'code',(SELECT c.note FROM relay_workspace_codes c WHERE c.workspace=w.id LIMIT 1),
              'suspended_ms',(to_jsonb(w)->>'suspended_ms')::bigint,'suspended_note',to_jsonb(w)->>'suspended_note',
              'agents_paused_ms',(to_jsonb(w)->>'agents_paused_ms')::bigint,
              'models',(SELECT COALESCE(jsonb_agg(DISTINCT a.model),'[]'::jsonb) FROM relay_shared_agents a WHERE a.workspace=w.id)) AS data
            FROM relay_workspaces w WHERE w.id=$3"), args).await?
            .context("No such workspace")?
            .try_get::<Value>("", "data")?;
        data["log"] = json!(json_rows(&self.db, "SELECT jsonb_build_object('summary',l.summary,'reason',l.reason,'at_ms',l.at_ms,'via',l.via,
            'by',COALESCE(u.display_name,u.email)) AS data FROM relay_operator_log l JOIN relay_users u ON u.id=l.actor
            WHERE l.area='workspaces' AND l.target=$1 ORDER BY l.id DESC LIMIT 20", vec![w.into()]).await?);
        Ok(data)
    }

    async fn workspace_name(tx: &DatabaseTransaction, w: &str) -> Result<String> {
        Ok(query(
            tx,
            "SELECT name FROM relay_workspaces WHERE id=$1",
            vec![w.into()],
        )
        .await?
        .context("No such workspace")?
        .try_get("", "name")?)
    }

    /// Refuses every member and stops all agent work until resumed.
    pub(crate) async fn operator_suspend(&self, by: &Operator, w: &str, why: &str) -> Result<()> {
        let tx = self.workspace_tx(w).await.context("No such workspace")?;
        let name = Self::workspace_name(&tx, w).await?;
        let changed = execute(&tx, "UPDATE relay_workspaces SET suspended_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint,
            suspended_note=$2,suspended_by=$3 WHERE id=$1 AND suspended_ms IS NULL", vec![w.into(), why.into(), by.user.id.clone().into()]).await?;
        ensure!(changed == 1, "{name} is already suspended");
        let mut stopped = 0;
        if !super::super::settings::pause::paused(&tx, w).await? {
            execute(&tx, "UPDATE relay_workspaces SET agents_paused_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint,
                agents_paused_by=$2,agents_paused_note=$3 WHERE id=$1", vec![w.into(), by.user.id.clone().into(), SUSPENDED_PAUSE.into()]).await?;
            for row in rows(&tx, "SELECT reply FROM relay_chat_jobs WHERE workspace=$1 AND state<>'finished' ORDER BY sequence", vec![w.into()]).await? {
                let reply: String = row.try_get("", "reply")?;
                stopped += super::super::assignments::stop_job(&tx, w, &reply, STOPPED, NOT_SENT).await? as usize;
            }
        }
        audit_with_summary(
            &tx,
            &by.user.id,
            Some(w),
            "workspace_suspended",
            w,
            &format!("Cuckoo suspended the workspace: {why}"),
        )
        .await?;
        super::super::settings::pause::announce(&tx, w).await?;
        record(
            &tx,
            by,
            "workspaces",
            "workspace_suspended",
            w,
            &format!("suspended {name} and stopped {stopped} running request(s)"),
            why,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Lets members back in. Agents resume only if suspension paused them.
    pub(crate) async fn operator_resume(&self, by: &Operator, w: &str, why: &str) -> Result<()> {
        let tx = self.workspace_tx(w).await.context("No such workspace")?;
        let name = Self::workspace_name(&tx, w).await?;
        let changed = execute(
            &tx,
            "UPDATE relay_workspaces SET suspended_ms=NULL,suspended_note=NULL,suspended_by=NULL
            WHERE id=$1 AND suspended_ms IS NOT NULL",
            vec![w.into()],
        )
        .await?;
        ensure!(changed == 1, "{name} is not suspended");
        execute(&tx, "UPDATE relay_workspaces SET agents_paused_ms=NULL,agents_paused_by=NULL,agents_paused_note=NULL
            WHERE id=$1 AND agents_paused_note=$2", vec![w.into(), SUSPENDED_PAUSE.into()]).await?;
        audit_with_summary(
            &tx,
            &by.user.id,
            Some(w),
            "workspace_resumed",
            w,
            "Cuckoo lifted the suspension",
        )
        .await?;
        super::super::settings::pause::announce(&tx, w).await?;
        record(
            &tx,
            by,
            "workspaces",
            "workspace_resumed",
            w,
            &format!("lifted the suspension of {name}"),
            why,
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

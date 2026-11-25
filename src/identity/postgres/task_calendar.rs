//! Activity › Calendar and the workspace calendar (designs/agent-tasks). The
//! calendar shows, for a window of at most six weeks, the runs agents made (done,
//! failed, no change, skipped, missed) and the runs they will make: every booked
//! run and the later runs of each repeating task's rule. An event trigger's runs
//! show as runs of its task (schema 148). Event triggers and paused tasks have no
//! time and are listed beside it. It shows the tasks of agents the
//! person looks after and the ones they asked for, and triggers that post where
//! they can read.
use super::activity::now::fixing::{FIXERS, exists};
use super::tasks::rules::{Rule, next_due};
use super::*;
use serde::Deserialize;
use serde_json::json;

const WINDOW_MS: i64 = 42 * 86_400_000;
/// Later runs drawn per repeating task.
const PER_TASK: usize = 60;
const POSTS_VISIBLY: &str = "(x.principal=$2 OR EXISTS(SELECT 1 FROM relay_chats c WHERE c.workspace=x.workspace AND c.id=x.chat
  AND (c.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id=$2))))";
const MY_TASK: &str = "(t.requester=$2 OR t.agent IN (SELECT id FROM f))";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Holiday {
    pub day: String,
    pub name: String,
}

/// Settings › General: working days (ISO, 1 = Monday), what a run due on a day off
/// does (next, before or skip) and the workspace's holidays.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkspaceCalendar {
    pub working_days: Vec<i64>,
    pub holiday_rule: String,
    pub holidays: Vec<Holiday>,
}

impl PgStore {
    pub(crate) async fn activity_calendar(
        &self,
        user: &User,
        w: &str,
        from: i64,
        to: i64,
        scope: Option<&str>,
    ) -> Result<Value> {
        super::activity::valid_scope(scope)?;
        ensure!(
            from > 0 && to > from && to - from <= WINDOW_MS,
            "Show at most six weeks at a time"
        );
        let tx = self.read_tx(w).await?;
        let admin = role(&tx, w, &user.id).await? == "Admin";
        let args = || -> Vec<sea_orm::Value> {
            vec![
                w.into(),
                user.id.clone().into(),
                admin.into(),
                scope.map(str::to_string).into(),
                from.into(),
                to.into(),
            ]
        };
        let zone = super::tasks::changes::workspace_zone(&tx, w).await?;
        let now = super::tasks::runs::now_ms(&tx).await?;
        let mut items = Vec::new();
        let mut untimed = Vec::new();
        if exists(&tx, "relay_task_notes").await? {
            items.extend(json_rows(&tx, &format!("{FIXERS} SELECT jsonb_build_object('kind','run','task',t.id,'title',t.title,'agent',t.agent,'agent_name',a.name,'at',k.due_ms,
                  'state',CASE WHEN k.state='skipped' THEN CASE WHEN k.detail LIKE 'Missed%' THEN 'missed' ELSE 'skipped' END
                   WHEN k.state='blocked' THEN 'blocked' WHEN k.quiet THEN 'no_change' WHEN m.state='complete' THEN 'done'
                   WHEN m.state IN ('failed','interrupted','access_changed') THEN 'failed' WHEN m.state='cancelled' THEN 'cancelled' ELSE 'working' END,
                  'detail',k.detail,'reply',k.reply) AS data
                FROM relay_task_wakeups k JOIN relay_tasks t ON t.workspace=k.workspace AND t.id=k.task
                JOIN relay_shared_agents a ON a.workspace=t.workspace AND a.id=t.agent
                LEFT JOIN relay_chat_messages m ON m.workspace=k.workspace AND m.id=k.reply
                WHERE k.workspace=$1 AND k.state IN ('started','skipped','blocked') AND k.kind<>'end' AND k.due_ms>=$5 AND k.due_ms<$6 AND {MY_TASK}
                ORDER BY k.due_ms LIMIT 500"), args()).await?);
            // Booked runs, and the later runs of each rule inside the window.
            let booked = rows(&tx, &format!("{FIXERS} SELECT t.id,t.title,t.agent,a.name AS agent_name,k.kind,k.due_ms,t.repeat,
                  COALESCE(t.timezone,ws.timezone,'Asia/Kolkata') AS zone,COALESCE(t.anchor_ms,t.created_ms) AS anchor_ms,t.until_ms,t.max_runs,t.runs
                FROM relay_task_wakeups k JOIN relay_tasks t ON t.workspace=k.workspace AND t.id=k.task
                JOIN relay_shared_agents a ON a.workspace=t.workspace AND a.id=t.agent JOIN relay_workspaces ws ON ws.id=t.workspace
                WHERE k.workspace=$1 AND k.state='pending' AND k.kind<>'end' AND k.due_ms<$6 AND t.outcome IS NULL AND {MY_TASK}
                ORDER BY k.due_ms LIMIT 200"), args()).await?;
            for row in booked {
                let base = json!({"kind": "planned", "task": row.try_get::<String>("", "id")?, "title": row.try_get::<String>("", "title")?,
                    "agent": row.try_get::<String>("", "agent")?, "agent_name": row.try_get::<String>("", "agent_name")?,
                    "run_kind": row.try_get::<String>("", "kind")?});
                let mut due: i64 = row.try_get("", "due_ms")?;
                let rule: Option<Value> = row.try_get("", "repeat")?;
                let rule: Option<Rule> = rule.map(serde_json::from_value).transpose()?;
                let (until, max_runs, runs): (Option<i64>, Option<i32>, i32) = (
                    row.try_get("", "until_ms")?,
                    row.try_get("", "max_runs")?,
                    row.try_get("", "runs")?,
                );
                let (zone, anchor): (String, i64) =
                    (row.try_get("", "zone")?, row.try_get("", "anchor_ms")?);
                let mut count = runs + 1;
                for _ in 0..PER_TASK {
                    if due >= from.max(now - 60_000) {
                        let mut item = base.clone();
                        item["at"] = json!(due);
                        items.push(item);
                    }
                    let Some(rule) = rule.as_ref().filter(|_| base["run_kind"] == "repeat") else {
                        break;
                    };
                    if max_runs.is_some_and(|m| count >= m) {
                        break;
                    }
                    match next_due(&tx, w, rule, &zone, anchor, due).await? {
                        Some(next) if next < to && until.is_none_or(|u| next <= u) => {
                            due = next;
                            count += 1;
                        }
                        _ => break,
                    }
                }
            }
            untimed.extend(json_rows(&tx, &format!("{FIXERS} SELECT jsonb_build_object('kind','paused','task',t.id,'title',t.title,'agent',t.agent,'agent_name',a.name,
                  'detail',t.paused_reason) AS data
                FROM relay_tasks t JOIN relay_shared_agents a ON a.workspace=t.workspace AND a.id=t.agent
                WHERE t.workspace=$1 AND t.outcome IS NULL AND t.paused_ms IS NOT NULL AND {MY_TASK} AND $5::bigint>0 AND $6::bigint>0
                ORDER BY t.paused_ms DESC LIMIT 50"), args()).await?);
        }
        let mine = "(x.agent IN (SELECT id FROM f) OR x.principal=$2)";
        // A trigger has no time: it is listed once, beside the calendar, linking to
        // its task (schema 148), whose runs show above as the task's runs.
        untimed.extend(json_rows(&tx, &format!("{FIXERS} SELECT jsonb_build_object('kind','trigger','trigger',x.id,'task',to_jsonb(x)->>'task','agent',x.agent,'agent_name',a.name,
              'title',COALESCE(x.input->>'name',x.source_definition->>'name'),'detail',x.source_definition->>'name') AS data
            FROM relay_event_triggers x JOIN relay_shared_agents a ON a.workspace=x.workspace AND a.id=x.agent
            WHERE x.workspace=$1 AND x.state='enabled' AND {mine} AND {POSTS_VISIBLY} AND $5::bigint>0 AND $6::bigint>0
            ORDER BY a.name,x.created_ms LIMIT 100"), args()).await?);
        tx.commit().await?;
        items.sort_by_key(|item| item["at"].as_i64().unwrap_or(0));
        Ok(
            json!({"from": from, "to": to, "now": now, "timezone": zone, "items": items, "untimed": untimed}),
        )
    }

    /// The workspace calendar; every member may read it.
    pub(crate) async fn workspace_calendar(&self, user: &User, w: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let admin = role(&tx, w, &user.id).await? == "Admin";
        let data = calendar_row(&tx, w).await?;
        tx.commit().await?;
        let mut data = data;
        data["editable"] = json!(admin);
        Ok(data)
    }

    /// Admins save working days, the day-off rule and holidays together.
    pub(crate) async fn save_workspace_calendar(
        &self,
        user: &User,
        w: &str,
        input: &WorkspaceCalendar,
    ) -> Result<Value> {
        let mut days = input.working_days.clone();
        days.sort_unstable();
        days.dedup();
        ensure!(
            !days.is_empty()
                && days.len() == input.working_days.len()
                && days.iter().all(|d| (1..=7).contains(d)),
            "Choose the working days, each once"
        );
        ensure!(
            ["next", "before", "skip"].contains(&input.holiday_rule.as_str()),
            "Choose what a run due on a day off does"
        );
        ensure!(input.holidays.len() <= 100, "Keep at most 100 holidays");
        let tx = self.workspace_tx(w).await?;
        manager(&tx, w, &user.id).await?;
        execute(
            &tx,
            "UPDATE relay_workspaces SET working_days=$2::smallint[],holiday_rule=$3 WHERE id=$1",
            vec![
                w.into(),
                json!(days)
                    .to_string()
                    .replace('[', "{")
                    .replace(']', "}")
                    .into(),
                input.holiday_rule.clone().into(),
            ],
        )
        .await?;
        execute(
            &tx,
            "DELETE FROM relay_workspace_holidays WHERE workspace=$1",
            vec![w.into()],
        )
        .await?;
        for holiday in &input.holidays {
            let name = holiday
                .name
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ");
            ensure!(
                (1..=80).contains(&name.chars().count()),
                "Name each holiday in at most 80 characters"
            );
            let day = holiday.day.trim();
            ensure!(
                day.len() == 10 && day.as_bytes()[4] == b'-' && day.as_bytes()[7] == b'-',
                "Give each holiday as a date like 2026-12-25"
            );
            let point = tx.begin().await?;
            let saved = execute(&point, "INSERT INTO relay_workspace_holidays(workspace,day,name) VALUES($1,$2::date,$3) ON CONFLICT (workspace,day) DO UPDATE SET name=excluded.name",
                vec![w.into(), day.into(), name.into()]).await;
            match saved {
                Ok(_) => point.commit().await?,
                Err(_) => {
                    point.rollback().await?;
                    anyhow::bail!("{day} is not a real date");
                }
            }
        }
        audit(&tx, &user.id, Some(w), "workspace_calendar_changed", w).await?;
        let data = calendar_row(&tx, w).await?;
        tx.commit().await?;
        Ok(data)
    }
}

async fn calendar_row(tx: &DatabaseTransaction, w: &str) -> Result<Value> {
    Ok(query(tx, "SELECT jsonb_build_object('timezone',COALESCE(w.timezone,'Asia/Kolkata'),'working_days',to_jsonb(w.working_days),'holiday_rule',w.holiday_rule,
          'holidays',COALESCE((SELECT jsonb_agg(jsonb_build_object('day',h.day::text,'name',h.name) ORDER BY h.day) FROM relay_workspace_holidays h WHERE h.workspace=w.id),'[]')) AS data
        FROM relay_workspaces w WHERE w.id=$1", vec![w.into()])
        .await?
        .context("Workspace unavailable")?
        .try_get("", "data")?)
}

#[cfg(test)]
#[path = "task_calendar_tests.rs"]
mod tests;

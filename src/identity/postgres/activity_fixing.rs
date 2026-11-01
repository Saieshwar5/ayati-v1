//! Needs fixing and Coming up (people-and-agents-first, Phase 4 AC3, AC7). For the
//! people who can fix an agent (its owner, managers and admins; My agents: the ones
//! they manage), what is wrong across their agents, grouped by cause with one fix
//! each, and what runs or wakes in the next 24 hours.
use super::super::super::*;
use serde_json::json;

#[path = "supervising_migration.rs"]
pub(in crate::identity::postgres) mod migration;
#[cfg(test)]
#[path = "activity_fixing_tests.rs"]
mod tests;

/// The agents this person can fix: $1 workspace, $2 person, $3 admin, $4 scope.
const FIXERS: &str = "WITH f AS (SELECT a.id,a.name FROM relay_shared_agents a WHERE a.workspace=$1 AND a.revision>0
 AND ($3 OR a.owner_id=$2 OR EXISTS(SELECT 1 FROM relay_agent_managers g WHERE g.workspace=a.workspace AND g.agent=a.id AND g.user_id=$2))
 AND ($4::text IS DISTINCT FROM 'mine' OR EXISTS(SELECT 1 FROM relay_agent_managers g WHERE g.workspace=a.workspace AND g.agent=a.id AND g.user_id=$2)))";
const NOW: &str = "(extract(epoch FROM clock_timestamp())*1000)::bigint";

/// Each cause: rows of agent, agent_name, ref, detail, at.
fn causes() -> Vec<(&'static str, String)> {
    vec![
        ("reconnect", "SELECT f.id AS agent,f.name AS agent_name,c.id AS ref,COALESCE(c.definition->>'name',c.server) AS detail,r.since AS at
          FROM f JOIN relay_agent_connections ac ON ac.workspace=$1 AND ac.agent=f.id
          JOIN relay_connector_reconnects r ON r.workspace=$1 AND r.connection=ac.connection
          JOIN relay_control_connections c ON c.workspace=$1 AND c.id=r.connection".into()),
        ("unconfirmed", "SELECT f.id,f.name,a.job,left(a.title,140),a.updated*1000 FROM f
          JOIN relay_chat_jobs j ON j.workspace=$1 AND j.payload->>'agent'=f.id
          JOIN relay_control_actions a ON a.workspace=$1 AND a.job=j.reply
          WHERE a.state='uncertain' OR (a.state='executing' AND a.updated<floor(extract(epoch FROM clock_timestamp()))::bigint-60)".into()),
        ("failing", format!("SELECT f.id,f.name,(array_agg(j.reply ORDER BY j.sequence DESC))[1],count(*)::text,max(j.created_ms) FROM f
          JOIN relay_chat_jobs j ON j.workspace=$1 AND j.payload->>'agent'=f.id AND j.created_ms>{NOW}-86400000
          JOIN relay_chat_messages m ON m.workspace=j.workspace AND m.id=j.reply AND m.state IN ('failed','interrupted')
          GROUP BY f.id,f.name HAVING count(*)>=2")),
        ("schedule", "SELECT f.id,f.name,s.id,s.name||CASE WHEN s.detail<>'' THEN ': '||s.detail ELSE '' END,s.checked_ms FROM f
          JOIN relay_agent_schedules s ON s.workspace=$1 AND s.agent=f.id AND s.state='blocked'".into()),
        ("trigger", "SELECT f.id,f.name,t.id,COALESCE(t.input->>'name','Event trigger')||CASE WHEN COALESCE(t.detail,'')<>'' THEN ': '||t.detail ELSE '' END,0::bigint FROM f
          JOIN relay_event_triggers t ON t.workspace=$1 AND t.agent=f.id AND t.state='blocked'".into()),
        ("waiting", format!("SELECT f.id,f.name,q.reply,left(q.specification->>'title',140),q.created_ms FROM f
          JOIN relay_chat_jobs j ON j.workspace=$1 AND j.payload->>'agent'=f.id AND j.state<>'finished'
          JOIN relay_task_questions q ON q.workspace=j.workspace AND q.reply=j.reply AND q.state='pending' AND q.created_ms<{NOW}-14400000
          UNION ALL SELECT f.id,f.name,a.job,left(a.title,140),a.created*1000 FROM f
          JOIN relay_chat_jobs j ON j.workspace=$1 AND j.payload->>'agent'=f.id
          JOIN relay_control_actions a ON a.workspace=$1 AND a.job=j.reply AND a.state='pending' AND a.requires_approval AND a.created*1000<{NOW}-14400000")),
        // Running far longer than this agent usually takes (at least ten minutes).
        ("slow", format!("SELECT f.id,f.name,j.reply,NULL::text,j.started_ms FROM f
          JOIN relay_chat_jobs j ON j.workspace=$1 AND j.payload->>'agent'=f.id AND j.state='running' AND j.started_ms IS NOT NULL
          WHERE {NOW}-j.started_ms>GREATEST(600000,3*COALESCE((SELECT percentile_cont(0.5) WITHIN GROUP (ORDER BY x.d) FROM
            (SELECT y.finished_ms-y.started_ms AS d FROM relay_chat_jobs y WHERE y.workspace=$1 AND y.payload->>'agent'=f.id
             AND y.state='finished' AND y.started_ms IS NOT NULL AND y.finished_ms IS NOT NULL ORDER BY y.sequence DESC LIMIT 30) x),0))")),
        ("reported", format!("SELECT f.id,f.name,n.id,n.cause||'|'||n.job||'|'||n.detail||COALESCE('|'||n.fix,''),n.created_ms FROM f
          JOIN relay_agent_needs n ON n.workspace=$1 AND n.agent=f.id AND n.dismissed_ms IS NULL AND n.created_ms>{NOW}-86400000")),
        ("tasks", "SELECT f.id,f.name,t.id,t.title,t.created_ms FROM f
          JOIN relay_tasks t ON t.workspace=$1 AND t.agent=f.id AND t.outcome IS NULL
          WHERE relay_task_status(t.workspace,t.id)='needs_fixing'".into()),
    ]
}

async fn exists(db: &impl ConnectionTrait, name: &str) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regclass($1) IS NOT NULL AS ok",
        vec![name.into()],
    )
    .await?
    .is_some_and(|r| r.try_get::<bool>("", "ok").unwrap_or(false)))
}

const NEEDS: [(&str, &str); 5] = [
    ("connection", "an account reconnected"),
    ("permission", "a permission"),
    ("information", "information"),
    ("external", "an outside system fixed"),
    ("other", "a person"),
];

/// Needs fixing, grouped by cause with one fix each.
pub(in crate::identity::postgres) async fn groups(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    admin: bool,
    scope: Option<&str>,
) -> Result<Vec<Value>> {
    let mut out = vec![];
    for (cause, sql) in causes() {
        let table = match cause {
            "reported" => Some("relay_agent_needs"),
            "tasks" => Some("relay_tasks"),
            _ => None,
        };
        if let Some(table) = table
            && !exists(tx, table).await?
        {
            continue;
        }
        let rows = rows(tx, &format!("{FIXERS} SELECT * FROM ({sql}) x(agent,agent_name,ref,detail,at) ORDER BY at DESC NULLS LAST LIMIT 50"),
            vec![w.into(), user.id.clone().into(), admin.into(), scope.map(str::to_string).into()]).await?;
        let items: Vec<Value> = rows.iter().map(|r| -> Result<Value> {
            Ok(json!({"agent": r.try_get::<String>("", "agent")?, "agent_name": r.try_get::<String>("", "agent_name")?,
                "ref": r.try_get::<String>("", "ref")?, "detail": r.try_get::<Option<String>>("", "detail")?, "at": r.try_get::<Option<i64>>("", "at")?}))
        }).collect::<Result<_>>()?;
        if items.is_empty() {
            continue;
        }
        let split = |key: &str| -> Vec<Vec<Value>> {
            let mut keys: Vec<String> = vec![];
            for i in &items {
                let k = i[key].as_str().unwrap_or_default().to_string();
                if !keys.contains(&k) {
                    keys.push(k);
                }
            }
            keys.iter()
                .map(|k| {
                    items
                        .iter()
                        .filter(|i| i[key] == k.as_str())
                        .cloned()
                        .collect()
                })
                .collect()
        };
        let agents = |list: &[Value]| -> usize {
            let mut a: Vec<&str> = list.iter().filter_map(|i| i["agent"].as_str()).collect();
            a.sort();
            a.dedup();
            a.len()
        };
        let plural = |n: usize, one: &str, many: &str| {
            if n == 1 {
                format!("1 {one}")
            } else {
                format!("{n} {many}")
            }
        };
        match cause {
            "reconnect" => {
                for list in split("ref") {
                    let n = agents(&list);
                    out.push(group(cause, format!("{} needs reconnecting · {}", list[0]["detail"].as_str().unwrap_or("An account"), plural(n, "agent stopped", "agents stopped")),
                    json!({"kind": "settings", "label": if admin { "Reconnect" } else { "Ask an admin to reconnect" }, "ref": list[0]["ref"]}), list));
                }
            }
            "unconfirmed" => out.push(group(
                cause,
                format!(
                    "{} couldn't be confirmed",
                    plural(items.len(), "action", "actions")
                ),
                json!({"kind": "request", "label": "Check it", "ref": items[0]["ref"]}),
                items,
            )),
            "failing" => out.push(group(
                cause,
                format!(
                    "{} failing repeatedly today",
                    plural(items.len(), "agent", "agents")
                ),
                json!({"kind": "request", "label": "How to fix it", "ref": items[0]["ref"]}),
                items,
            )),
            "schedule" => out.push(group(
                cause,
                format!("{} stopped", plural(items.len(), "routine", "routines")),
                json!({"kind": "agent", "label": "Review routines", "ref": items[0]["agent"]}),
                items,
            )),
            "trigger" => out.push(group(
                cause,
                format!(
                    "{} stopped",
                    plural(items.len(), "event trigger", "event triggers")
                ),
                json!({"kind": "agent", "label": "Review triggers", "ref": items[0]["agent"]}),
                items,
            )),
            "waiting" => out.push(group(
                cause,
                format!(
                    "{} waiting on a person for over 4 hours",
                    plural(items.len(), "request", "requests")
                ),
                json!({"kind": "needs", "label": "Open Needs me"}),
                items,
            )),
            "slow" => out.push(group(
                cause,
                format!(
                    "{} running much longer than usual",
                    plural(items.len(), "request", "requests")
                ),
                json!({"kind": "request", "label": "See what it's doing", "ref": items[0]["ref"]}),
                items,
            )),
            "reported" => {
                let items: Vec<Value> = items
                    .into_iter()
                    .map(|mut i| {
                        let text = i["detail"].as_str().unwrap_or_default().to_string();
                        let mut parts = text.splitn(4, '|');
                        i["cause"] = json!(parts.next());
                        i["request"] = json!(parts.next());
                        i["detail"] = json!(parts.next());
                        i["fix"] = json!(parts.next());
                        i
                    })
                    .collect();
                for (key, need) in NEEDS {
                    let list: Vec<Value> = items
                        .iter()
                        .filter(|i| i["cause"] == key)
                        .cloned()
                        .collect();
                    if list.is_empty() {
                        continue;
                    }
                    out.push(group(cause, format!("{} {need}", plural(agents(&list), "agent needs", "agents need")),
                        json!({"kind": "need", "label": "See what's needed", "ref": list[0]["ref"]}), list));
                }
            }
            _ => out.push(group(
                cause,
                format!("{} need fixing", plural(items.len(), "task", "tasks")),
                json!({"kind": "tasks", "label": "Open Tasks"}),
                items,
            )),
        }
    }
    Ok(out)
}

fn group(cause: &str, title: String, fix: Value, items: Vec<Value>) -> Value {
    let mut agents: Vec<Value> = vec![];
    for i in &items {
        if !agents.iter().any(|a| a["id"] == i["agent"]) {
            agents.push(json!({"id": i["agent"], "name": i["agent_name"]}));
        }
    }
    json!({"cause": cause, "title": title, "fix": fix, "agents": agents, "count": items.len(), "items": items.into_iter().take(20).collect::<Vec<_>>()})
}

/// What runs or wakes in the next 24 hours: routines, follow-ups and due tasks.
pub(in crate::identity::postgres) async fn coming_up(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    admin: bool,
    scope: Option<&str>,
) -> Result<Vec<Value>> {
    let args = || -> Vec<sea_orm::Value> {
        vec![
            w.into(),
            user.id.clone().into(),
            admin.into(),
            scope.map(str::to_string).into(),
        ]
    };
    let soon = format!("BETWEEN {NOW} AND {NOW}+86400000");
    let mut items = json_rows(tx, &format!("{FIXERS} SELECT jsonb_build_object('kind','routine','id',s.id,'agent',s.agent,'agent_name',a.name,
          'title',s.name,'at',s.next_ms,'revision',s.revision,'can_pause',true) AS data
        FROM relay_agent_schedules s JOIN relay_shared_agents a ON a.workspace=s.workspace AND a.id=s.agent
        WHERE s.workspace=$1 AND s.state='enabled' AND s.next_ms {soon}
          AND NOT EXISTS(SELECT 1 FROM relay_home_schedule_bindings b WHERE b.workspace=s.workspace AND b.schedule=s.id)
          AND (s.agent IN (SELECT id FROM f) OR s.principal=$2)
        ORDER BY s.next_ms LIMIT 50"), args()).await?;
    if exists(tx, "relay_task_wakeups").await? {
        items.extend(json_rows(tx, &format!("{FIXERS} SELECT jsonb_build_object('kind',CASE WHEN k.id IS NULL THEN 'due' ELSE 'follow_up' END,'id',t.id,'agent',t.agent,'agent_name',a.name,
              'title',t.title,'at',COALESCE(k.due_ms,t.due_ms),'can_pause',k.id IS NOT NULL) AS data
            FROM relay_tasks t JOIN relay_shared_agents a ON a.workspace=t.workspace AND a.id=t.agent
            LEFT JOIN relay_task_wakeups k ON k.workspace=t.workspace AND k.task=t.id AND k.state='pending' AND k.due_ms {soon}
            WHERE t.workspace=$1 AND t.outcome IS NULL AND (k.id IS NOT NULL OR t.due_ms {soon})
              AND (t.requester=$2 OR t.agent IN (SELECT id FROM f))
            ORDER BY 1 LIMIT 50"), args()).await?);
    }
    items.sort_by_key(|i| i["at"].as_i64().unwrap_or(i64::MAX));
    Ok(items)
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Problem {
    pub cause: String,
    pub detail: String,
    pub fix: Option<String>,
}

impl PgStore {
    /// `report_problem`: the agent says what it needs from a person, in a fixed
    /// shape, so Needs fixing can group it with others of the same cause.
    pub(crate) async fn report_agent_need(
        &self,
        job: &crate::identity::chat::model::Job,
        input: &Problem,
    ) -> Result<Value> {
        ensure!(
            NEEDS.iter().any(|(c, _)| *c == input.cause),
            "cause is connection, permission, information, external or other"
        );
        let detail = input.detail.trim();
        ensure!(
            !detail.is_empty() && detail.chars().count() <= 300,
            "Say what is needed in at most 300 characters"
        );
        let fix = input
            .fix
            .as_deref()
            .map(str::trim)
            .filter(|f| !f.is_empty());
        ensure!(
            fix.is_none_or(|f| f.chars().count() <= 200),
            "Keep the fix to 200 characters"
        );
        let tx = self.workspace_tx(&job.workspace).await?;
        super::super::super::control_actions::authorize(&tx, job, true).await?;
        ensure!(
            exists(&tx, "relay_agent_needs").await?,
            "Reporting needs requires the latest workspace database"
        );
        let open = query(
            &tx,
            "SELECT count(*) AS n FROM relay_agent_needs WHERE workspace=$1 AND job=$2",
            vec![job.workspace.clone().into(), job.reply.clone().into()],
        )
        .await?
        .context("Count")?
        .try_get::<i64>("", "n")?;
        ensure!(open < 5, "This request already reported five needs");
        execute(&tx, "INSERT INTO relay_agent_needs(workspace,id,agent,job,cause,detail,fix) VALUES($1,$2,$3,$4,$5,$6,$7)",
            vec![job.workspace.clone().into(), uuid::Uuid::new_v4().to_string().into(), job.agent.clone().into(), job.reply.clone().into(),
                input.cause.clone().into(), detail.into(), fix.map(str::to_string).into()]).await?;
        tx.commit().await?;
        Ok(
            json!({"state": "reported", "message": "Saved for the people who can fix it (Activity → Needs fixing). Tell the person in one line what you need and stop; don't retry the same thing."}),
        )
    }

    /// A manager, owner or admin dismisses something an agent reported.
    pub(crate) async fn dismiss_need(&self, user: &User, w: &str, id: &str) -> Result<()> {
        let tx = self.workspace_tx(w).await?;
        let admin = role(&tx, w, &user.id).await? == "Admin";
        let n = execute(&tx, &format!("{FIXERS} UPDATE relay_agent_needs SET dismissed_by=$2,dismissed_ms={NOW} WHERE workspace=$1 AND id=$5 AND dismissed_ms IS NULL AND agent IN (SELECT id FROM f)"),
            vec![w.into(), user.id.clone().into(), admin.into(), None::<String>.into(), id.into()]).await?;
        ensure!(n == 1, "Nothing to dismiss");
        audit(&tx, &user.id, Some(w), "agent_need_dismissed", id).await?;
        tx.commit().await?;
        Ok(())
    }
}

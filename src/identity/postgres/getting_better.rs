//! Getting better over time (people-and-agents-first, Phase 5), for the people who
//! can fix an agent (owner, managers, admins):
//!
//! - **Measures** per agent and per person: tasks done, failed, on time, how many
//!   needed a person, person-minutes (time from reading a question or approval to
//!   answering it, at most ten minutes each), requests and model runs (counts, not
//!   money: there are no per-call token counts yet).
//! - **Decisions become rules:** five or more approvals of the same action for the same
//!   agent in 30 days and no rejection: offer "let the agent do this without asking".
//!   A yes saves the agent's rule through the ordinary path, so it can never be looser
//!   than the admin's rule; it's shown on the Tools tab and can be undone there.
//! - **Goals** per agent (name, target, unit, week or month) with progress and risk
//!   the agent reports (`my_goals`, `update_goal`).
//! - **Weekly review:** a weekly routine in which the agent writes what it did, what
//!   went wrong, what it learned and one suggested change (`write_review`).
use super::super::super::*;
use crate::identity::chat::model::Job;
use serde::Deserialize;
use serde_json::json;

#[path = "getting_better_migration.rs"]
pub(in crate::identity::postgres) mod migration;
#[cfg(test)]
#[path = "getting_better_tests.rs"]
mod tests;

/// The agents this person can fix: $1 workspace, $2 person, $3 admin, $4 scope.
const FIXERS: &str = "WITH f AS (SELECT a.id,a.name FROM relay_shared_agents a WHERE a.workspace=$1 AND a.revision>0
 AND ($3 OR a.owner_id=$2 OR EXISTS(SELECT 1 FROM relay_agent_managers g WHERE g.workspace=a.workspace AND g.agent=a.id AND g.user_id=$2))
 AND ($4::text IS DISTINCT FROM 'mine' OR EXISTS(SELECT 1 FROM relay_agent_managers g WHERE g.workspace=a.workspace AND g.agent=a.id AND g.user_id=$2)))";
const NOW: &str = "(extract(epoch FROM clock_timestamp())*1000)::bigint";

async fn available(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(db, "SELECT to_regclass('relay_agent_goals') IS NOT NULL AND to_regclass('relay_tasks') IS NOT NULL AS ok", vec![]).await?
        .is_some_and(|r| r.try_get::<bool>("", "ok").unwrap_or(false)))
}

fn measures_sql(by: &str) -> String {
    let (key, name, join) = match by {
        "agent" => ("f.id", "f.name", "LEFT JOIN t ON t.agent=f.id"),
        _ => (
            "u.id",
            "COALESCE(u.display_name,u.email)",
            "JOIN t ON t.agent=f.id JOIN relay_users u ON u.id=t.requester",
        ),
    };
    format!("{FIXERS}, t AS (
      SELECT t.*,relay_task_status(t.workspace,t.id) AS status FROM relay_tasks t
      WHERE t.workspace=$1 AND t.agent IN (SELECT id FROM f) AND t.created_ms>{NOW}-$5::bigint*86400000 AND t.outcome IS DISTINCT FROM 'not_task'
    ), spent AS (
      SELECT j.task,LEAST(GREATEST(q.answered_ms-COALESCE(r.read_ms,q.created_ms),0),600000) AS ms FROM relay_task_questions q
        JOIN relay_chat_jobs j ON j.workspace=q.workspace AND j.reply=q.reply
        LEFT JOIN relay_attention_receipts r ON r.workspace=q.workspace AND r.kind='question' AND r.source=q.id AND r.user_id=q.answered_by
        WHERE q.workspace=$1 AND q.answered_ms IS NOT NULL AND j.task IS NOT NULL
      UNION ALL
      SELECT j.task,LEAST(GREATEST(a.decided_at*1000-COALESCE(r.read_ms,a.created*1000),0),600000) FROM relay_control_actions a
        JOIN relay_chat_jobs j ON j.workspace=a.workspace AND j.reply=a.job
        LEFT JOIN relay_attention_receipts r ON r.workspace=a.workspace AND r.kind='approval' AND r.source=a.id AND r.user_id=a.decided_by
        WHERE a.workspace=$1 AND a.decided_by IS NOT NULL AND j.task IS NOT NULL
    )
    SELECT jsonb_build_object('id',{key},'name',{name},'tasks',count(t.id),
      'done',count(t.id) FILTER (WHERE t.status='done'),'failed',count(t.id) FILTER (WHERE t.status='failed'),
      'open',count(t.id) FILTER (WHERE t.status NOT IN ('done','failed','cancelled')),
      'due_done',count(t.id) FILTER (WHERE t.status='done' AND t.due_ms IS NOT NULL),
      'on_time',count(t.id) FILTER (WHERE t.status='done' AND t.due_ms IS NOT NULL AND t.closed_ms<=t.due_ms),
      'needed_person',count(t.id) FILTER (WHERE t.id IN (SELECT task FROM spent)),
      'person_minutes',round(COALESCE(sum((SELECT sum(s.ms) FROM spent s WHERE s.task=t.id)),0)/60000.0,1)) AS data
    FROM f {join} GROUP BY {key},{name} ORDER BY {name} LIMIT 100")
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Goal {
    pub agent: String,
    pub id: Option<String>,
    pub name: String,
    pub target: f64,
    #[serde(default)]
    pub unit: String,
    pub period: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct GoalProgress {
    pub goal: String,
    pub current: f64,
    pub risk: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Review {
    pub done: String,
    pub went_wrong: String,
    pub learned: String,
    pub suggestion: Option<String>,
}

fn line(text: &str, max: usize, what: &str) -> Result<String> {
    let text = text.trim();
    ensure!(
        !text.is_empty() && text.chars().count() <= max,
        "Write {what} in at most {max} characters"
    );
    Ok(text.to_string())
}

const REVIEW: &str = "Write your weekly review for your managers. Look back over the last seven days of your requests and tasks, then call write_review once: done (what you finished), went_wrong (what failed or needed a person, and why), learned (what you'd do differently) and, if useful, suggestion (one change to your instructions or skills for a manager to approve). Plain, short sentences; no new work.";

impl PgStore {
    /// Activity → Review: measures, rule suggestions, goals and weekly reviews.
    pub(crate) async fn activity_review(
        &self,
        user: &User,
        w: &str,
        days: i64,
        scope: Option<&str>,
    ) -> Result<Value> {
        ensure!((1..=90).contains(&days), "Choose 1 to 90 days");
        let tx = self.read_tx(w).await?;
        if !available(&tx).await? {
            tx.commit().await?;
            return Ok(json!({"available": false}));
        }
        let admin = role(&tx, w, &user.id).await? == "Admin";
        let args = || -> Vec<sea_orm::Value> {
            vec![
                w.into(),
                user.id.clone().into(),
                admin.into(),
                scope.map(str::to_string).into(),
                days.into(),
            ]
        };
        let agents = json_rows(&tx, &measures_sql("agent"), args()).await?;
        let people = json_rows(&tx, &measures_sql("person"), args()).await?;
        let runs = json_rows(&tx, &format!("{FIXERS} SELECT jsonb_build_object('id',f.id,'requests',count(j.reply),'model_runs',COALESCE(sum(j.attempts),0)) AS data
            FROM f LEFT JOIN relay_chat_jobs j ON j.workspace=$1 AND j.payload->>'agent'=f.id AND j.created_ms>{NOW}-$5::bigint*86400000 GROUP BY f.id"), args()).await?;
        let agents: Vec<Value> = agents
            .into_iter()
            .map(|mut a| {
                if let Some(r) = runs.iter().find(|r| r["id"] == a["id"]) {
                    a["requests"] = r["requests"].clone();
                    a["model_runs"] = r["model_runs"].clone();
                }
                a
            })
            .collect();
        let mut suggestions = vec![];
        for s in json_rows(&tx, &format!("{FIXERS} SELECT jsonb_build_object('agent',f.id,'agent_name',f.name,'connection',a.connection,'operation',a.operation,'title',max(a.title),
              'approved',count(*) FILTER (WHERE a.decision='approve')) AS data
            FROM relay_control_actions a JOIN relay_chat_jobs j ON j.workspace=a.workspace AND j.reply=a.job JOIN f ON f.id=j.payload->>'agent'
            WHERE a.workspace=$1 AND a.decided_by IS NOT NULL AND a.decided_at>floor(extract(epoch FROM clock_timestamp()))::bigint-2592000
              AND NOT EXISTS(SELECT 1 FROM relay_rule_suggestions_dismissed d WHERE d.workspace=$1 AND d.agent=f.id AND d.connection=a.connection AND d.operation=a.operation AND d.user_id=$2)
              AND NOT EXISTS(SELECT 1 FROM relay_control_rules r WHERE r.workspace=$1 AND r.agent=f.id AND r.connection=a.connection AND r.operation=a.operation AND r.mode='allow')
            GROUP BY f.id,f.name,a.connection,a.operation
            HAVING count(*) FILTER (WHERE a.decision='approve')>=5 AND count(*) FILTER (WHERE a.decision IS DISTINCT FROM 'approve')=0
            ORDER BY 1 LIMIT 20"), args()[..4].to_vec()).await? {
            // Never offer what the admin's rule wouldn't allow.
            let ceiling = super::super::super::connector_rules::workspace_mode(&tx, w, s["connection"].as_str().unwrap_or_default(), s["operation"].as_str().unwrap_or_default()).await;
            if ceiling.is_ok_and(|m| m == "allow") {
                suggestions.push(s);
            }
        }
        let goals = json_rows(&tx, &format!("{FIXERS} SELECT jsonb_build_object('id',g.id,'agent',g.agent,'agent_name',f.name,'name',g.name,'target',g.target,'unit',g.unit,
              'period',g.period,'current',g.current,'risk',g.risk,'updated_ms',g.updated_ms) AS data
            FROM relay_agent_goals g JOIN f ON f.id=g.agent WHERE g.workspace=$1 AND NOT g.archived ORDER BY f.name,g.created_ms"), args()[..4].to_vec()).await?;
        let reviews = json_rows(&tx, &format!("{FIXERS} SELECT jsonb_build_object('id',r.id,'agent',r.agent,'agent_name',f.name,'done',r.done,'went_wrong',r.went_wrong,
              'learned',r.learned,'suggestion',r.suggestion,'at',r.created_ms,'request',r.job) AS data
            FROM relay_agent_reviews r JOIN f ON f.id=r.agent WHERE r.workspace=$1 AND r.created_ms>{NOW}-$5::bigint*86400000
            ORDER BY r.created_ms DESC LIMIT 30"), args()).await?;
        let weekly = json_rows(&tx, &format!("{FIXERS} SELECT jsonb_build_object('agent',f.id,'enabled',s.state='enabled','schedule',s.id,'next_ms',s.next_ms) AS data
            FROM relay_review_routines rr JOIN f ON f.id=rr.agent JOIN relay_agent_schedules s ON s.workspace=rr.workspace AND s.id=rr.schedule WHERE rr.workspace=$1"), args()[..4].to_vec()).await?;
        tx.commit().await?;
        Ok(
            json!({"available": true, "days": days, "agents": agents, "people": people, "suggestions": suggestions, "goals": goals, "reviews": reviews, "weekly": weekly}),
        )
    }

    async fn fixer(
        &self,
        tx: &DatabaseTransaction,
        user: &User,
        w: &str,
        agent: &str,
    ) -> Result<()> {
        let admin = role(tx, w, &user.id).await? == "Admin";
        ensure!(
            query(
                tx,
                &format!("{FIXERS} SELECT id FROM f WHERE id=$5"),
                vec![
                    w.into(),
                    user.id.clone().into(),
                    admin.into(),
                    None::<String>.into(),
                    agent.into()
                ]
            )
            .await?
            .is_some(),
            "Only the agent's owner, managers and admins can do this"
        );
        Ok(())
    }

    /// Yes to a suggested rule: the agent does this action without asking. Saved
    /// through the ordinary rule path, so never looser than the admin's rule.
    pub(crate) async fn accept_rule_suggestion(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        connection: &str,
        operation: &str,
    ) -> Result<()> {
        let revision = query(&self.db, "SELECT revision FROM relay_control_rules WHERE workspace=$1 AND agent=$2 AND connection=$3 AND operation=$4",
            vec![w.into(), agent.into(), connection.into(), operation.into()]).await?
            .map(|r| r.try_get::<i64>("", "revision")).transpose()?.unwrap_or(0);
        self.control_rule(
            user,
            w,
            agent,
            &crate::identity::control::model::Rule {
                connection: connection.into(),
                operation: operation.into(),
                mode: "allow".into(),
                revision,
            },
        )
        .await
    }

    pub(crate) async fn dismiss_rule_suggestion(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        connection: &str,
        operation: &str,
    ) -> Result<()> {
        let tx = self.workspace_tx(w).await?;
        self.fixer(&tx, user, w, agent).await?;
        execute(&tx, "INSERT INTO relay_rule_suggestions_dismissed(workspace,agent,connection,operation,user_id) VALUES($1,$2,$3,$4,$5) ON CONFLICT DO NOTHING",
            vec![w.into(), agent.into(), connection.into(), operation.into(), user.id.clone().into()]).await?;
        tx.commit().await?;
        Ok(())
    }

    /// A manager sets (or changes) one of the agent's goals.
    pub(crate) async fn save_goal(&self, user: &User, w: &str, input: &Goal) -> Result<Value> {
        let name = line(&input.name, 80, "the goal")?;
        ensure!(
            input.target.is_finite() && input.target > 0.0,
            "The target is a positive number"
        );
        ensure!(
            input.unit.chars().count() <= 16,
            "Keep the unit to 16 characters"
        );
        ensure!(
            ["week", "month"].contains(&input.period.as_str()),
            "A goal is per week or per month"
        );
        let tx = self.workspace_tx(w).await?;
        ensure!(
            available(&tx).await?,
            "Goals need the latest workspace database"
        );
        self.fixer(&tx, user, w, &input.agent).await?;
        let id = match &input.id {
            Some(id) => {
                ensure!(execute(&tx, "UPDATE relay_agent_goals SET name=$4,target=$5,unit=$6,period=$7 WHERE workspace=$1 AND id=$2 AND agent=$3 AND NOT archived",
                    vec![w.into(), id.clone().into(), input.agent.clone().into(), name.into(), input.target.into(), input.unit.trim().into(), input.period.clone().into()]).await? == 1, "Goal unavailable");
                id.clone()
            }
            None => {
                let n = query(&tx, "SELECT count(*) AS n FROM relay_agent_goals WHERE workspace=$1 AND agent=$2 AND NOT archived", vec![w.into(), input.agent.clone().into()]).await?
                    .context("Count")?.try_get::<i64>("", "n")?;
                ensure!(n < 5, "An agent has at most five goals");
                let id = uuid::Uuid::new_v4().to_string();
                execute(&tx, "INSERT INTO relay_agent_goals(workspace,id,agent,name,target,unit,period,created_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8)",
                    vec![w.into(), id.clone().into(), input.agent.clone().into(), name.into(), input.target.into(), input.unit.trim().into(), input.period.clone().into(), user.id.clone().into()]).await?;
                id
            }
        };
        audit(&tx, &user.id, Some(w), "agent_goal_saved", &id).await?;
        tx.commit().await?;
        Ok(json!({"id": id}))
    }

    pub(crate) async fn archive_goal(&self, user: &User, w: &str, id: &str) -> Result<()> {
        let tx = self.workspace_tx(w).await?;
        let agent: String = query(
            &tx,
            "SELECT agent FROM relay_agent_goals WHERE workspace=$1 AND id=$2",
            vec![w.into(), id.into()],
        )
        .await?
        .context("Goal unavailable")?
        .try_get("", "agent")?;
        self.fixer(&tx, user, w, &agent).await?;
        execute(
            &tx,
            "UPDATE relay_agent_goals SET archived=true WHERE workspace=$1 AND id=$2",
            vec![w.into(), id.into()],
        )
        .await?;
        audit(&tx, &user.id, Some(w), "agent_goal_archived", id).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Weekly review on or off: a weekly routine for the agent, run as the manager
    /// who turned it on, in its own work room.
    pub(crate) async fn set_weekly_review(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        enabled: bool,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        ensure!(
            available(&tx).await?,
            "Weekly reviews need the latest workspace database"
        );
        self.fixer(&tx, user, w, agent).await?;
        let current = query(&tx, "SELECT s.id,s.revision,s.state FROM relay_review_routines rr JOIN relay_agent_schedules s ON s.workspace=rr.workspace AND s.id=rr.schedule WHERE rr.workspace=$1 AND rr.agent=$2",
            vec![w.into(), agent.into()]).await?;
        tx.commit().await?;
        if let Some(row) = current {
            let id: String = row.try_get("", "id")?;
            let state: String = row.try_get("", "state")?;
            if (state == "enabled") != enabled {
                self.change_schedule(
                    user,
                    w,
                    agent,
                    &id,
                    &crate::identity::automations::model::Change {
                        request: uuid::Uuid::new_v4().to_string(),
                        revision: row.try_get("", "revision")?,
                        enabled,
                    },
                )
                .await?;
            }
            return Ok(json!({"schedule": id, "enabled": enabled}));
        }
        ensure!(enabled, "Weekly review is already off");
        let zone = query(
            &self.db,
            "SELECT to_jsonb(w)->>'timezone' AS tz FROM relay_workspaces w WHERE id=$1",
            vec![w.into()],
        )
        .await?
        .and_then(|r| r.try_get::<Option<String>>("", "tz").ok().flatten())
        .unwrap_or_else(|| "UTC".into());
        let id = uuid::Uuid::new_v4().to_string();
        let schedule = crate::identity::automations::model::Schedule {
            id: id.clone(),
            chat: None,
            name: "Weekly review".into(),
            content: REVIEW.into(),
            first_ms: (now() + 7 * 86400) * 1000,
            interval_seconds: Some(7 * 86400),
            timezone: zone,
        };
        self.save_schedule(user, w, agent, &schedule).await?;
        let tx = self.workspace_tx(w).await?;
        execute(&tx, "INSERT INTO relay_review_routines(workspace,agent,schedule) VALUES($1,$2,$3) ON CONFLICT(workspace,agent) DO UPDATE SET schedule=excluded.schedule",
            vec![w.into(), agent.into(), id.clone().into()]).await?;
        tx.commit().await?;
        Ok(json!({"schedule": id, "enabled": true}))
    }

    /// Home (HP7): questions this person keeps asking an agent, which a panel could
    /// answer on Home instead. Only their own messages; at most three.
    pub(crate) async fn panel_suggestions(&self, user: &User, w: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        role(&tx, w, &user.id).await?;
        let items = json_rows(&tx, "SELECT jsonb_build_object('agent',x.agent,'agent_name',a.name,'ask',x.sample,'count',x.n) AS data FROM (
              SELECT m.target AS agent,lower(regexp_replace(trim(m.content),'[[:punct:][:space:]]+$','')) AS ask,count(*) AS n,max(m.content) AS sample
              FROM relay_chat_messages m WHERE m.workspace=$1 AND m.kind='user' AND m.author=$2 AND m.target IS NOT NULL
                AND m.at>floor(extract(epoch FROM clock_timestamp()))::bigint-1209600 AND char_length(m.content) BETWEEN 8 AND 160
                AND m.content ~* '^\\s*(how many|how much|what is|what''s|whats|show|list|total|status of)'
              GROUP BY 1,2 HAVING count(*)>=3) x
            JOIN relay_shared_agents a ON a.workspace=$1 AND a.id=x.agent
            WHERE relay_agent_can_talk($1,x.agent,$2) OR a.owner_id=$2
            ORDER BY x.n DESC LIMIT 3", vec![w.into(), user.id.clone().into()]).await?;
        tx.commit().await?;
        Ok(json!({"suggestions": items}))
    }

    /// `my_goals`: the agent's goals and how far it has reported.
    pub(crate) async fn my_goals(&self, job: &Job) -> Result<Value> {
        let tx = self.read_tx(&job.workspace).await?;
        super::super::super::control_actions::authorize(&tx, job, true).await?;
        let goals = if available(&tx).await? {
            json_rows(&tx, "SELECT jsonb_build_object('id',id,'name',name,'target',target,'unit',unit,'period',period,'current',current,'risk',risk) AS data FROM relay_agent_goals WHERE workspace=$1 AND agent=$2 AND NOT archived ORDER BY created_ms",
                vec![job.workspace.clone().into(), job.agent.clone().into()]).await?
        } else {
            vec![]
        };
        tx.commit().await?;
        Ok(json!({"goals": goals}))
    }

    /// `update_goal`: progress on one of the agent's goals, and a risk raised early.
    pub(crate) async fn update_goal(&self, job: &Job, input: &GoalProgress) -> Result<Value> {
        ensure!(
            input.current.is_finite() && input.current >= 0.0,
            "Report progress as a number of zero or more"
        );
        let risk = input
            .risk
            .as_deref()
            .map(|r| line(r, 280, "the risk"))
            .transpose()?;
        let tx = self.workspace_tx(&job.workspace).await?;
        super::super::super::control_actions::authorize(&tx, job, true).await?;
        ensure!(
            available(&tx).await?,
            "Goals need the latest workspace database"
        );
        ensure!(execute(&tx, &format!("UPDATE relay_agent_goals SET current=$4,risk=$5,updated_ms={NOW} WHERE workspace=$1 AND id=$2 AND agent=$3 AND NOT archived"),
            vec![job.workspace.clone().into(), input.goal.clone().into(), job.agent.clone().into(), input.current.into(), risk.into()]).await? == 1, "That isn't one of your goals; call my_goals");
        tx.commit().await?;
        Ok(json!({"state": "saved"}))
    }

    /// `write_review`: the agent's weekly review, once per request.
    pub(crate) async fn write_review(&self, job: &Job, input: &Review) -> Result<Value> {
        let done = line(&input.done, 1000, "what you did")?;
        let wrong = line(&input.went_wrong, 1000, "what went wrong")?;
        let learned = line(&input.learned, 1000, "what you learned")?;
        let suggestion = input
            .suggestion
            .as_deref()
            .map(|s| line(s, 1000, "the suggestion"))
            .transpose()?;
        let tx = self.workspace_tx(&job.workspace).await?;
        super::super::super::control_actions::authorize(&tx, job, true).await?;
        ensure!(
            available(&tx).await?,
            "Reviews need the latest workspace database"
        );
        execute(&tx, "INSERT INTO relay_agent_reviews(workspace,id,agent,job,done,went_wrong,learned,suggestion) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(workspace,job) DO NOTHING",
            vec![job.workspace.clone().into(), uuid::Uuid::new_v4().to_string().into(), job.agent.clone().into(), job.reply.clone().into(), done.into(), wrong.into(), learned.into(), suggestion.into()]).await?;
        tx.commit().await?;
        Ok(json!({"state": "saved", "message": "Saved for your managers (Activity → Progress)."}))
    }
}

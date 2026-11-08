//! Agent computers' lifecycle (schema 103): one computer per agent, held by one request
//! at a time, created on first use, woken when needed and put to sleep when idle. Each
//! awake stretch is charged at the size's price. See runtime/agent-computers.md.
use super::*;
use crate::identity::agents::computer_settings::ComputerSettings;
use crate::identity::chat::model::Job;
use crate::identity::computers::{Awake, Call, Routed, Spec};
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use serde_json::json;

/// A request keeps the computer this long after its last use; others wait until then.
const HOLD_SECONDS: i64 = 180;

/// Schema 103: agent computers.
pub(super) struct ComputersMigration;
impl MigrationName for ComputersMigration {
    fn name(&self) -> &str {
        "m20261108_000103_computers"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for ComputersMigration {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("computers.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Agent computers require a forward migration".into(),
        ))
    }
}

/// An awake computer held for one request, ready for commands and files.
pub(super) struct Ready {
    pub routed: Routed,
    pub key: String,
    pub awake: Awake,
}
impl Ready {
    pub fn call(&self) -> Call<'_> {
        Call {
            api: &self.routed.api,
            computers: &self.routed.computers,
            key: &self.key,
        }
    }
}

pub(super) fn refused(message: &str) -> Value {
    json!({"state": "denied", "message": message})
}
pub(super) fn unavailable(message: &str) -> Value {
    json!({"state": "unavailable", "message": message})
}

/// The agent's computer settings, or none when the agent doesn't exist.
pub(super) async fn agent_computer(
    db: &impl ConnectionTrait,
    w: &str,
    agent: &str,
) -> Result<Option<ComputerSettings>> {
    let Some(row) = query(db, "SELECT settings->'computer' AS computer FROM relay_shared_agents WHERE workspace=$1 AND id=$2", vec![w.into(), agent.into()]).await? else {
        return Ok(None);
    };
    let value: Option<Value> = row.try_get("", "computer")?;
    Ok(Some(
        value
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default(),
    ))
}

/// Millionths of a dollar for `seconds` at a price per hour.
fn micro(seconds: i64, usd_per_hour: f64) -> i64 {
    (seconds.max(0) as f64 * usd_per_hour * 1e6 / 3600.0).round() as i64
}

/// What the agent did on its computer for one request, as Activity steps: each command
/// with its exit code and the start of its output, and each file listed, read or saved.
pub(super) async fn steps(db: &impl ConnectionTrait, w: &str, reply: &str) -> Result<Vec<Value>> {
    if !super::model_budgets::present(db, "relay_computer_steps").await? {
        return Ok(vec![]);
    }
    json_rows(db, "SELECT jsonb_build_object('id','computer-'||s.position,
        'title',CASE s.kind WHEN 'command' THEN 'Ran: '||left(s.detail,160) WHEN 'list' THEN 'Looked in '||s.detail
          WHEN 'read' THEN 'Read '||s.detail WHEN 'browser' THEN s.detail ELSE 'Saved '||s.detail END,
        'effect','computer','state',s.state,'automatic',true,'decision',NULL,'decided_by',NULL,'account',NULL,'arguments','{}'::jsonb,
        'computer',jsonb_build_object('kind',s.kind,'detail',s.detail,'exit_code',s.exit_code,'output',left(COALESCE(s.output,''),20000),
          'output_bytes',s.output_bytes,'milliseconds',s.milliseconds),'at',s.at) AS data
        FROM relay_computer_steps s WHERE s.workspace=$1 AND s.reply=$2 ORDER BY s.position LIMIT 200", vec![w.into(), reply.into()]).await
}

impl PgStore {
    /// Computers are set up on this Cuckoo and switched on for the agent.
    pub(crate) async fn computer_on(&self, w: &str, agent: &str) -> Result<bool> {
        Ok(self.computers.is_some()
            && agent_computer(&self.db, w, agent)
                .await?
                .is_some_and(|c| c.on))
    }

    /// An agent's computer can be turned on only where computers are set up; one already
    /// on stays saveable if they are taken away.
    pub(super) fn computer_allowed(
        &self,
        input: &crate::identity::agents::config::SaveAgent,
        was_on: bool,
    ) -> Result<()> {
        ensure!(
            !input.settings.computer.on || was_on || self.computers.is_some(),
            "Computers are not set up on this Cuckoo yet"
        );
        Ok(())
    }

    /// Holds the agent's computer for this request and makes sure it is awake with the
    /// agent's current internet rule, or says why it can't be used now.
    pub(super) async fn computer_ready(
        &self,
        job: &Job,
    ) -> Result<std::result::Result<Ready, Value>> {
        let Some(computers) = self.computers.clone() else {
            return Ok(Err(unavailable(
                "Computers are not set up on this Cuckoo. Answer without one.",
            )));
        };
        let Some(settings) = agent_computer(&self.db, &job.workspace, &job.agent).await? else {
            return Ok(Err(refused("Agent unavailable")));
        };
        if !settings.on {
            return Ok(Err(refused(
                "Your computer is switched off. Tell the person its owner can turn it on in your Environment tab.",
            )));
        }
        if let Some(reason) =
            super::model_budgets::budget_reached(&self.db, &job.workspace, &job.agent).await?
        {
            return Ok(Err(refused(&reason)));
        }
        let file = computers.file()?;
        let Some(size) = file.size(&settings.size) else {
            return Ok(Err(unavailable(
                "This computer size is not offered now. Tell the person to choose another size in your Environment tab.",
            )));
        };
        let route = &size.routes[0];
        let at = now();
        let tx = self.db.begin().await?;
        // A computer not made yet follows the size chosen now; one already made keeps its
        // size until it is reset.
        execute(&tx, "INSERT INTO relay_computers(workspace,agent,size,provider,image,created_at) VALUES($1,$2,$3,$4,$5,$6)
            ON CONFLICT(workspace,agent) DO UPDATE SET size=EXCLUDED.size,provider=EXCLUDED.provider,image=EXCLUDED.image
            WHERE relay_computers.provider_id IS NULL AND relay_computers.state='asleep'",
            vec![job.workspace.clone().into(), job.agent.clone().into(), settings.size.clone().into(), route.provider.clone().into(), route.image.clone().into(), at.into()]).await?;
        let row = query(&tx, "SELECT size,provider,image,provider_id,state,internet,held_by,held_until FROM relay_computers WHERE workspace=$1 AND agent=$2 FOR UPDATE",
            vec![job.workspace.clone().into(), job.agent.clone().into()]).await?.context("Computer unavailable")?;
        let held_by: Option<String> = row.try_get("", "held_by")?;
        let held_until: i64 = row.try_get("", "held_until")?;
        if held_by.as_deref().is_some_and(|r| r != job.reply) && held_until > at {
            return Ok(Err(unavailable(
                "Your computer is busy with another request. Try again in a minute, or answer without it.",
            )));
        }
        let state: String = row.try_get("", "state")?;
        if state == "deleting" {
            return Ok(Err(unavailable(
                "Your computer is being reset. Try again in a minute.",
            )));
        }
        execute(&tx, "UPDATE relay_computers SET held_by=$3,held_until=$4,last_used=$5 WHERE workspace=$1 AND agent=$2",
            vec![job.workspace.clone().into(), job.agent.clone().into(), job.reply.clone().into(), (at + HOLD_SECONDS).into(), at.into()]).await?;
        tx.commit().await?;

        let provider: String = row.try_get("", "provider")?;
        let image: String = row.try_get("", "image")?;
        let mut provider_id: Option<String> = row.try_get("", "provider_id")?;
        let applied: String = row.try_get("", "internet")?;
        let routed = file.routed(&provider)?;
        let key = routed.key()?;
        let call = Call {
            api: &routed.api,
            computers: &routed.computers,
            key: &key,
        };
        let internet = settings.internet();
        let wanted = format!("{internet:?}");
        let awake_seconds = (u64::from(file.sleep_after_minutes) + 5) * 60;
        // A new stretch of awake time starts unless the computer is already awake.
        let woke = state != "awake";
        let existing = match &provider_id {
            Some(id) => match computers.fresh(id).filter(|_| !woke) {
                Some(awake) => Ok(Some(awake)),
                None => routed
                    .provider
                    .wake(&call, id, awake_seconds)
                    .await
                    .map(Some),
            },
            None => Ok(None),
        };
        // A computer the provider lost is started again, empty, straight away.
        let existing = match existing {
            Err(error) if error.to_string().contains("no longer exists") => {
                provider_id = None;
                Ok(None)
            }
            other => other,
        };
        let created = matches!(existing, Ok(None));
        let label = format!("{}/{}", job.workspace, job.agent);
        let attempt = match existing {
            Ok(Some(awake)) => Ok(awake),
            Ok(None) => {
                let spec = Spec {
                    image: &image,
                    awake_seconds,
                    internet: &internet,
                    label: &label,
                };
                routed.provider.create(&call, &spec).await
            }
            Err(error) => Err(error),
        };
        let awake = match attempt {
            Ok(awake) => awake,
            Err(error) => {
                // The state stays as it was, so an open awake stretch is still closed and
                // charged by the sweep.
                let error = error.to_string();
                execute(&self.db, "UPDATE relay_computers SET problem=$3,provider_id=$4,held_by=NULL,held_until=0 WHERE workspace=$1 AND agent=$2",
                    vec![job.workspace.clone().into(), job.agent.clone().into(), error.chars().take(400).collect::<String>().into(), provider_id.into()]).await?;
                tracing::warn!("An agent computer could not be started: {error}");
                return Ok(Err(unavailable(
                    "Your computer could not be started just now. Try again in a minute, or answer without it.",
                )));
            }
        };
        computers.remember(&awake);
        if created {
            provider_id = Some(awake.id.clone());
        } else if applied != wanted {
            routed
                .provider
                .set_internet(&call, &awake.id, &internet)
                .await?;
        }
        execute(&self.db, "UPDATE relay_computers SET provider_id=$3,state='awake',internet=$4,awake_since=COALESCE(awake_since,$5),problem=NULL WHERE workspace=$1 AND agent=$2",
            vec![job.workspace.clone().into(), job.agent.clone().into(), provider_id.into(), wanted.into(), at.into()]).await?;
        if woke {
            execute(&self.db, "INSERT INTO relay_computer_sessions(workspace,agent,reply,person,size,provider,woke_at) VALUES($1,$2,$3,$4,$5,$6,$7)",
                vec![job.workspace.clone().into(), job.agent.clone().into(), job.reply.clone().into(), job.requester.clone().into(), row.try_get::<String>("", "size")?.into(), provider.into(), at.into()]).await?;
        }
        Ok(Ok(Ready { routed, key, awake }))
    }

    /// Keeps the computer held a while longer after a use.
    pub(super) async fn computer_used(&self, job: &Job) -> Result<()> {
        let at = now();
        execute(&self.db, "UPDATE relay_computers SET last_used=$4,held_until=$5 WHERE workspace=$1 AND agent=$2 AND held_by=$3",
            vec![job.workspace.clone().into(), job.agent.clone().into(), job.reply.clone().into(), at.into(), (at + HOLD_SECONDS).into()]).await?;
        Ok(())
    }

    /// Puts idle computers to sleep, charges their awake time, and deletes computers being
    /// reset or whose agent is gone. Run by a controller task every half minute.
    pub(crate) async fn computers_sweep(&self) -> Result<usize> {
        let Some(computers) = self.computers.clone() else {
            return Ok(0);
        };
        let file = computers.file()?;
        let at = now();
        let idle = at - i64::from(file.sleep_after_minutes) * 60;
        let longest = at - i64::from(file.longest_work_minutes) * 60;
        let due = rows(
            &self.db,
            "SELECT c.workspace,c.agent,c.size,c.provider,c.image,c.provider_id,c.state,
              (a.id IS NULL) AS orphaned FROM relay_computers c
              LEFT JOIN relay_shared_agents a ON a.workspace=c.workspace AND a.id=c.agent
              WHERE c.state='deleting' OR a.id IS NULL
                 OR (c.state='awake' AND ((c.held_until<$1 AND c.last_used<$2) OR c.awake_since<$3))
              LIMIT 50",
            vec![at.into(), idle.into(), longest.into()],
        )
        .await?;
        let mut done = 0;
        for row in due {
            let w: String = row.try_get("", "workspace")?;
            let agent: String = row.try_get("", "agent")?;
            let provider: String = row.try_get("", "provider")?;
            let id: Option<String> = row.try_get("", "provider_id")?;
            let removing = row.try_get::<String>("", "state")? == "deleting"
                || row.try_get::<bool>("", "orphaned")?;
            let outcome = async {
                let routed = file.routed(&provider)?;
                let key = routed.key()?;
                let call = Call {
                    api: &routed.api,
                    computers: &routed.computers,
                    key: &key,
                };
                if let Some(id) = &id {
                    computers.forget(id);
                    if removing {
                        routed.provider.delete(&call, id).await?;
                    } else {
                        routed.provider.sleep(&call, id).await?;
                    }
                }
                anyhow::Ok(())
            }
            .await;
            if let Err(error) = outcome {
                tracing::warn!(
                    "An agent computer could not be put to sleep or deleted; it will retry: {error}"
                );
                continue;
            }
            let size = file.size(&row.try_get::<String>("", "size")?).cloned();
            let image: String = row.try_get("", "image")?;
            let price = size.as_ref().map_or(0.0, |s| s.usd_per_hour);
            let cost = size
                .as_ref()
                .and_then(|s| {
                    s.routes
                        .iter()
                        .find(|r| r.provider == provider && r.image == image)
                })
                .map_or(0.0, |r| r.usd_per_hour);
            let open = rows(&self.db, "SELECT id,woke_at FROM relay_computer_sessions WHERE workspace=$1 AND agent=$2 AND slept_at IS NULL", vec![w.clone().into(), agent.clone().into()]).await?;
            for session in open {
                let seconds = at - session.try_get::<i64>("", "woke_at")?;
                execute(&self.db, "UPDATE relay_computer_sessions SET slept_at=$3,seconds=$4,charge_micro=$5,cost_micro=$6 WHERE workspace=$1 AND id=$2",
                    vec![w.clone().into(), session.try_get::<i64>("", "id")?.into(), at.into(), seconds.max(0).into(), micro(seconds, price).into(), micro(seconds, cost).into()]).await?;
            }
            if removing {
                execute(
                    &self.db,
                    "DELETE FROM relay_computers WHERE workspace=$1 AND agent=$2",
                    vec![w.into(), agent.into()],
                )
                .await?;
            } else {
                execute(&self.db, "UPDATE relay_computers SET state='asleep',awake_since=NULL,held_by=NULL,held_until=0 WHERE workspace=$1 AND agent=$2", vec![w.into(), agent.into()]).await?;
            }
            done += 1;
        }
        Ok(done)
    }

    /// The agent's computer for its Environment tab: the setting, the computer's state and
    /// this month's awake time and charge. Anyone who can see the agent may read it.
    pub(crate) async fn computer_status(&self, user: &User, w: &str, agent: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        super::agent_profiles::managed_access(&tx, &user.id, w, agent, false).await?;
        let settings = agent_computer(&tx, w, agent).await?.unwrap_or_default();
        let computer = query(&tx, "SELECT size,state,awake_since,last_used,created_at,problem,held_by FROM relay_computers WHERE workspace=$1 AND agent=$2", vec![w.into(), agent.into()]).await?;
        let month = query(&tx, "WITH zone AS (SELECT COALESCE(to_jsonb(w)->>'timezone','UTC') AS z FROM relay_workspaces w WHERE w.id=$1),
              start AS (SELECT extract(epoch FROM (date_trunc('month', now() AT TIME ZONE z) AT TIME ZONE z))::bigint AS s FROM zone)
              SELECT COALESCE(sum(CASE WHEN slept_at IS NULL THEN extract(epoch FROM now())::bigint-woke_at ELSE seconds END),0)::bigint AS seconds,
                     COALESCE(sum(charge_micro),0)::bigint AS charge
              FROM relay_computer_sessions, start WHERE workspace=$1 AND agent=$2 AND woke_at>=start.s", vec![w.into(), agent.into()]).await?.context("Usage unavailable")?;
        tx.commit().await?;
        let computer = match computer {
            None => Value::Null,
            Some(row) => json!({
                "size": row.try_get::<String>("", "size")?,
                "state": row.try_get::<String>("", "state")?,
                "awake_since": row.try_get::<Option<i64>>("", "awake_since")?,
                "last_used": row.try_get::<i64>("", "last_used")?,
                "created_at": row.try_get::<i64>("", "created_at")?,
                "problem": row.try_get::<Option<String>>("", "problem")?,
                "in_use": row.try_get::<Option<String>>("", "held_by")?.is_some(),
            }),
        };
        let file = self.computers.as_ref().map(|c| c.file()).transpose()?;
        Ok(json!({
            "configured": file.is_some(),
            "settings": settings,
            "computer": computer,
            "month": {"seconds": month.try_get::<i64>("", "seconds")?, "charge_usd": month.try_get::<i64>("", "charge")? as f64 / 1e6},
            "sleep_after_minutes": file.as_ref().map(|f| f.sleep_after_minutes),
        }))
    }

    /// Wipes the agent's computer back to new: the provider deletes it, and the next use
    /// starts a fresh one at the current size. The agent's owner or an admin only.
    pub(crate) async fn computer_reset(&self, user: &User, w: &str, agent: &str) -> Result<()> {
        let tx = self.workspace_tx(w).await?;
        let profile = super::agent_profiles::managed_access(&tx, &user.id, w, agent, true).await?;
        let changed = execute(&tx, "UPDATE relay_computers SET state='deleting',held_by=NULL,held_until=0 WHERE workspace=$1 AND agent=$2", vec![w.into(), agent.into()]).await?;
        if changed > 0 {
            let name = profile["name"].as_str().unwrap_or("the agent");
            audit_with_summary(
                &tx,
                &user.id,
                Some(w),
                "computer_reset",
                agent,
                &format!("Reset {name}'s computer"),
            )
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Lets other requests use the computer as soon as this one finishes.
    pub(super) async fn computer_release(
        db: &impl ConnectionTrait,
        w: &str,
        reply: &str,
    ) -> Result<()> {
        let present = query(
            db,
            "SELECT to_regclass('relay_computers') IS NOT NULL AS present",
            vec![],
        )
        .await?
        .map(|r| r.try_get::<bool>("", "present"))
        .transpose()?
        .unwrap_or(false);
        // A person using the browser in a hand-over keeps the computer for this request.
        if present {
            execute(db, "UPDATE relay_computers SET held_by=NULL,held_until=0 WHERE workspace=$1 AND held_by=$2
                AND NOT EXISTS (SELECT 1 FROM relay_task_questions q WHERE q.workspace=$1 AND q.reply=$2
                  AND q.request_key LIKE 'browser-hand-over-%' AND q.state='pending')", vec![w.into(), reply.into()]).await?;
        }
        Ok(())
    }
}

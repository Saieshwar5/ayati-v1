//! Steering running work and routines proposed in Chat (people-and-agents-first,
//! Phase 4 AC6, T3).
//!
//! - **Add to a running request:** the person who asked adds a note ("also use the new
//!   price list"). It reaches the agent with the result of its next tool call
//!   (`person_added`), instead of waiting behind it as a new request. Anything the
//!   agent never saw is carried into a new request when it finishes.
//! - **Routines from Chat:** `propose_schedule` saves a draft on the agent's reply; it
//!   becomes a routine only when the person who asked confirms it ("Save routine"),
//!   with the ordinary routine checks.
use super::super::super::*;
use crate::identity::chat::model::{Job, NewMessage};
use serde_json::json;

#[path = "steering_migration.rs"]
pub(in crate::identity::postgres) mod migration;
#[cfg(test)]
#[path = "steering_tests.rs"]
mod tests;

async fn available(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regclass('relay_job_steers') IS NOT NULL AS ok",
        vec![],
    )
    .await?
    .is_some_and(|r| r.try_get::<bool>("", "ok").unwrap_or(false)))
}

/// What the person added since the agent last looked; marked as seen.
pub(in crate::identity::postgres) async fn take(
    db: &impl ConnectionTrait,
    job: &Job,
) -> Result<Vec<Value>> {
    if !available(db).await? {
        return Ok(vec![]);
    }
    json_rows(db, "UPDATE relay_job_steers s SET read_ms=(extract(epoch FROM clock_timestamp())*1000)::bigint
        FROM relay_users u WHERE s.workspace=$1 AND s.job=$2 AND s.read_ms IS NULL AND s.carried IS NULL AND u.id=s.author
        RETURNING jsonb_build_object('text',s.content,'from',COALESCE(u.display_name,u.email),'at',s.created_ms) AS data",
        vec![job.workspace.clone().into(), job.reply.clone().into()]).await
}

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Proposal {
    pub name: String,
    pub instructions: String,
    pub first_in_minutes: i64,
    pub every_minutes: Option<i64>,
}

impl PgStore {
    /// For the tool broker: what the person added, delivered with the agent's next step.
    pub(crate) async fn take_steers(&self, job: &Job) -> Result<Vec<Value>> {
        take(&self.db, job).await
    }

    /// The person who asked adds to their running request.
    pub(crate) async fn steer_request(
        &self,
        user: &User,
        w: &str,
        reply: &str,
        text: &str,
    ) -> Result<Value> {
        let text = text.trim();
        ensure!(
            !text.is_empty() && text.chars().count() <= 2000,
            "Write what to add in at most 2,000 characters"
        );
        let tx = self.workspace_tx(w).await?;
        ensure!(
            available(&tx).await?,
            "Adding to a running request needs the latest workspace database"
        );
        let row = query(&tx, "SELECT j.state,j.payload->>'requester' AS requester FROM relay_chat_jobs j WHERE j.workspace=$1 AND j.reply=$2 FOR UPDATE", vec![w.into(), reply.into()]).await?
            .context("Request unavailable")?;
        ensure!(
            row.try_get::<String>("", "requester")? == user.id,
            "Only the person who asked can add to this request"
        );
        ensure!(
            row.try_get::<String>("", "state")? != "finished",
            "This request already finished; ask the agent in chat instead"
        );
        let n = query(
            &tx,
            "SELECT count(*) AS n FROM relay_job_steers WHERE workspace=$1 AND job=$2",
            vec![w.into(), reply.into()],
        )
        .await?
        .context("Count")?
        .try_get::<i64>("", "n")?;
        ensure!(n < 20, "This request already has 20 additions");
        execute(
            &tx,
            "INSERT INTO relay_job_steers(workspace,id,job,author,content) VALUES($1,$2,$3,$4,$5)",
            vec![
                w.into(),
                uuid::Uuid::new_v4().to_string().into(),
                reply.into(),
                user.id.clone().into(),
                text.into(),
            ],
        )
        .await?;
        audit(&tx, &user.id, Some(w), "request_steered", reply).await?;
        tx.commit().await?;
        Ok(json!({"added": true, "message": "Added. The agent reads it at its next step."}))
    }

    /// Add to the request a task is running now.
    pub(crate) async fn steer_task(
        &self,
        user: &User,
        w: &str,
        task: &str,
        text: &str,
    ) -> Result<Value> {
        let details = self.task_details(user, w, task).await?;
        let running = details["requests"].as_array().and_then(|r| {
            r.iter().rev().find(|s| {
                ["queued", "running", "waiting"].contains(&s["state"].as_str().unwrap_or_default())
            })
        });
        let reply = running
            .and_then(|s| s["id"].as_str())
            .context("Nothing is running for this task; ask the agent in chat instead")?
            .to_string();
        self.steer_request(user, w, &reply, text).await
    }

    /// What was added and whether the agent has seen it, for the request's details.
    pub(in crate::identity::postgres) async fn steers_for(
        &self,
        tx: &DatabaseTransaction,
        w: &str,
        reply: &str,
    ) -> Result<Vec<Value>> {
        if !available(tx).await? {
            return Ok(vec![]);
        }
        json_rows(tx, "SELECT jsonb_build_object('text',s.content,'from',COALESCE(u.display_name,u.email),'at',s.created_ms,'seen',s.read_ms IS NOT NULL,'carried',s.carried) AS data
            FROM relay_job_steers s JOIN relay_users u ON u.id=s.author WHERE s.workspace=$1 AND s.job=$2 ORDER BY s.created_ms", vec![w.into(), reply.into()]).await
    }

    /// When a request finishes, anything the agent never saw becomes a new request
    /// from the same person, so nothing added is lost.
    pub(in crate::identity::postgres) async fn carry_steers(
        &self,
        tx: &DatabaseTransaction,
        job: &Job,
    ) -> Result<()> {
        if !self.pooled || !available(tx).await? {
            return Ok(());
        }
        let unseen = rows(tx, "SELECT id,content FROM relay_job_steers WHERE workspace=$1 AND job=$2 AND read_ms IS NULL AND carried IS NULL ORDER BY created_ms",
            vec![job.workspace.clone().into(), job.reply.clone().into()]).await?;
        if unseen.is_empty() {
            return Ok(());
        }
        let text: Vec<String> = unseen
            .iter()
            .filter_map(|r| r.try_get::<String>("", "content").ok())
            .collect();
        let input = NewMessage {
            reply_to: None,
            id: uuid::Uuid::new_v4().to_string(),
            content: format!(
                "Added while you were working on my last request:\n{}",
                text.join("\n")
            ),
            agent: Some(job.agent.clone()),
            thread: None,
            attachments: vec![],
        };
        let person = User {
            id: job.requester.clone(),
            email: String::new(),
            display_name: None,
        };
        let intake = tx.begin().await?;
        match self
            .background_in(&intake, &person, &job.workspace, &job.chat, &input)
            .await
        {
            Ok(Some(next)) => {
                intake.commit().await?;
                execute(tx, "UPDATE relay_job_steers SET carried=$3 WHERE workspace=$1 AND job=$2 AND read_ms IS NULL AND carried IS NULL",
                    vec![job.workspace.clone().into(), job.reply.clone().into(), next.reply.into()]).await?;
            }
            Ok(None) => intake.rollback().await?,
            Err(error) => {
                intake.rollback().await?;
                if error.is::<sea_orm::DbErr>() {
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    /// `propose_schedule`: the agent drafts a routine; the person who asked confirms it.
    pub(crate) async fn propose_schedule(&self, job: &Job, input: &Proposal) -> Result<Value> {
        let name = input.name.trim();
        let instructions = input.instructions.trim();
        ensure!(
            !name.is_empty() && name.chars().count() <= 80,
            "Name the routine in at most 80 characters"
        );
        ensure!(
            !instructions.is_empty() && instructions.chars().count() <= 2000,
            "Give the routine's instructions in at most 2,000 characters"
        );
        ensure!(
            (1..=525_600).contains(&input.first_in_minutes),
            "The first run is 1 minute to a year away"
        );
        ensure!(
            input
                .every_minutes
                .is_none_or(|m| (60..=43_200).contains(&m)),
            "Repeat every hour to every 30 days, or not at all"
        );
        let tx = self.workspace_tx(&job.workspace).await?;
        super::super::super::control_actions::authorize(&tx, job, true).await?;
        ensure!(
            available(&tx).await?,
            "Proposing routines needs the latest workspace database"
        );
        let n = query(
            &tx,
            "SELECT count(*) AS n FROM relay_schedule_drafts WHERE workspace=$1 AND job=$2",
            vec![job.workspace.clone().into(), job.reply.clone().into()],
        )
        .await?
        .context("Count")?
        .try_get::<i64>("", "n")?;
        ensure!(n < 3, "This request already proposed three routines");
        let id = uuid::Uuid::new_v4().to_string();
        execute(&tx, "INSERT INTO relay_schedule_drafts(workspace,id,agent,job,chat,requester,name,content,first_ms,interval_seconds) VALUES($1,$2,$3,$4,$5,$6,$7,$8,(extract(epoch FROM clock_timestamp())*1000)::bigint+$9::bigint*60000,$10)",
            vec![job.workspace.clone().into(), id.clone().into(), job.agent.clone().into(), job.reply.clone().into(), job.chat.clone().into(), job.requester.clone().into(),
                name.into(), instructions.into(), input.first_in_minutes.into(), input.every_minutes.map(|m| m * 60).into()]).await?;
        tx.commit().await?;
        Ok(
            json!({"draft": id, "state": "proposed", "message": "Proposed. The person sees it under your reply and saves it with Save routine; nothing runs until they do. Say so in one line."}),
        )
    }

    /// The person who asked saves or dismisses a proposed routine.
    pub(crate) async fn decide_schedule_draft(
        &self,
        user: &User,
        w: &str,
        id: &str,
        save: bool,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        ensure!(
            available(&tx).await?,
            "Routines from Chat need the latest workspace database"
        );
        let row = query(
            &tx,
            "SELECT to_jsonb(d) AS d FROM relay_schedule_drafts d WHERE workspace=$1 AND id=$2",
            vec![w.into(), id.into()],
        )
        .await?
        .context("Proposal unavailable")?;
        let draft: Value = row.try_get("", "d")?;
        ensure!(
            draft["requester"] == user.id.as_str(),
            "Only the person who asked can save or dismiss this routine"
        );
        tx.commit().await?;
        if draft["state"] != "pending" {
            return Ok(json!({"state": draft["state"]}));
        }
        if save {
            let zone = query(
                &self.db,
                "SELECT to_jsonb(w)->>'timezone' AS tz FROM relay_workspaces w WHERE id=$1",
                vec![w.into()],
            )
            .await?
            .and_then(|r| r.try_get::<Option<String>>("", "tz").ok().flatten())
            .unwrap_or_else(|| "UTC".into());
            // The draft's id is the routine's id, so a retried save is the same routine.
            let schedule = crate::identity::automations::model::Schedule {
                id: id.to_string(),
                chat: draft["chat"].as_str().map(str::to_string),
                name: draft["name"].as_str().unwrap_or_default().into(),
                content: draft["content"].as_str().unwrap_or_default().into(),
                first_ms: draft["first_ms"].as_i64().unwrap_or_default(),
                interval_seconds: draft["interval_seconds"].as_i64(),
                timezone: zone,
            };
            self.save_schedule(
                user,
                w,
                draft["agent"].as_str().unwrap_or_default(),
                &schedule,
            )
            .await?;
        }
        let tx = self.workspace_tx(w).await?;
        execute(&tx, "UPDATE relay_schedule_drafts SET state=$3 WHERE workspace=$1 AND id=$2 AND state='pending'", vec![w.into(), id.into(), if save { "saved" } else { "dismissed" }.into()]).await?;
        audit(
            &tx,
            &user.id,
            Some(w),
            if save {
                "schedule_draft_saved"
            } else {
                "schedule_draft_dismissed"
            },
            id,
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"state": if save { "saved" } else { "dismissed" }}))
    }
}

/// Chat shows a proposed routine under the agent's reply.
pub(in crate::identity::postgres) async fn hydrate(
    tx: &DatabaseTransaction,
    w: &str,
    messages: &mut [Value],
) -> Result<()> {
    let ids: Vec<Value> = messages
        .iter()
        .filter(|m| m["kind"] == "agent")
        .map(|m| m["id"].clone())
        .collect();
    if ids.is_empty() || !available(tx).await? {
        return Ok(());
    }
    for row in rows(tx, "SELECT job,jsonb_build_object('id',id,'name',name,'first_ms',first_ms,'interval_seconds',interval_seconds,'state',state,'requester',requester) AS d
        FROM relay_schedule_drafts WHERE workspace=$1 AND job IN (SELECT jsonb_array_elements_text($2::jsonb)) ORDER BY created_ms", vec![w.into(), json!(ids).into()]).await? {
        let job: String = row.try_get("", "job")?;
        if let Some(m) = messages.iter_mut().find(|m| m["id"] == job.as_str()) {
            let list = m["routines"].as_array().cloned().unwrap_or_default();
            let mut list = list;
            list.push(row.try_get("", "d")?);
            m["routines"] = json!(list);
        }
    }
    Ok(())
}

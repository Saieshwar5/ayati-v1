//! Steering running work and routines proposed in Chat (people-and-agents-first,
//! Phase 4 AC6, T3).
//!
//! - **Add to a running request:** the person who asked adds a note ("also use the new
//!   price list"). It reaches the agent with the result of its next tool call
//!   (`person_added`), instead of waiting behind it as a new request. Anything the
//!   agent never saw is carried into a new request when it finishes.
//! - **Routines proposed in Chat (until schema 144)** are no longer shown or saved:
//!   agents create repeating tasks in chat (`create_task` with a rule), and schema
//!   147 made every saved schedule a task. Old drafts stay in `relay_schedule_drafts`.
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
            agents: vec![],
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
}

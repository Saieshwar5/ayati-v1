//! Agent configuration does not grant access to the work hidden in its conversations.
use super::*;
use crate::identity::chat::store::Job;
use serde_json::json;

impl PgStore {
    pub(crate) async fn agent_requests(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        mine: bool,
        active: bool,
        before: Option<i64>,
    ) -> Result<Value> {
        ensure!(before.is_none_or(|n| n > 0), "Invalid request cursor");
        let tx = self.read_tx(w).await?;
        super::agent_profiles::managed_access(&tx, &user.id, w, agent, false).await?;
        let records = rows(
            &tx,
            include_str!("agent_requests.sql"),
            vec![
                w.into(),
                agent.into(),
                user.id.clone().into(),
                mine.into(),
                active.into(),
                before.unwrap_or(i64::MAX).into(),
                Option::<String>::None.into(),
            ],
        )
        .await?;
        let more = records.len() > 25;
        let mut cursor = None;
        let mut requests = Vec::new();
        for row in records.into_iter().take(25) {
            cursor = Some(row.try_get::<i64>("", "sequence")?);
            if let Some(request) = project(&tx, user, w, agent, row).await? {
                requests.push(request);
            }
        }
        tx.commit().await?;
        Ok(json!({"requests":requests,"next_before":if more {cursor} else {None}}))
    }

    pub(crate) async fn agent_request(
        &self,
        user: &User,
        w: &str,
        agent: &str,
        reply: &str,
    ) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        super::agent_profiles::managed_access(&tx, &user.id, w, agent, false).await?;
        let row = query(
            &tx,
            include_str!("agent_requests.sql"),
            vec![
                w.into(),
                agent.into(),
                user.id.clone().into(),
                false.into(),
                false.into(),
                i64::MAX.into(),
                reply.into(),
            ],
        )
        .await?
        .context("Request unavailable")?;
        let request = project(&tx, user, w, agent, row)
            .await?
            .context("Request unavailable")?;
        tx.commit().await?;
        Ok(request)
    }
}

async fn project(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    agent: &str,
    row: QueryResult,
) -> Result<Option<Value>> {
    let job: Job = serde_json::from_value(row.try_get("", "payload")?)?;
    ensure!(
        job.workspace == w
            && job.agent == agent
            && job.reply == row.try_get::<String>("", "id")?
            && job.chat == row.try_get::<String>("", "chat")?,
        "Request scope mismatch"
    );
    let mut viewer = job.clone();
    viewer.requester = user.id.clone();
    if !super::source_access::allowed(tx, &viewer).await? {
        return Ok(None);
    }
    let current = super::controlled_attempts::access(tx, &job).await.is_ok();
    let job_state: String = row.try_get("", "job_state")?;
    let message_state: String = row.try_get("", "state")?;
    let progress: Option<Value> = row.try_get("", "progress")?;
    let phase = progress.as_ref().and_then(|v| v["state"].as_str());
    let state = if job_state == "finished" {
        message_state.as_str()
    } else if !current {
        "access_changed"
    } else if job_state == "waiting" {
        phase.unwrap_or("waiting")
    } else if job_state == "queued" {
        if phase == Some("resuming") {
            "resuming"
        } else {
            "queued"
        }
    } else if row.try_get::<i64>("", "lease_until")? <= now() {
        "recovering"
    } else if row
        .try_get::<Option<String>>("", "attempt_state")?
        .as_deref()
        == Some("launching")
        || phase == Some("starting")
    {
        "preparing"
    } else {
        "running"
    };
    let latest = if current || job_state == "finished" {
        progress
            .as_ref()
            .and_then(|p| p["text"].as_str())
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .unwrap_or(row.try_get("", "content")?)
    } else {
        "Request access changed.".into()
    };
    let approvals = if current {
        json_rows(tx,"SELECT jsonb_build_object('id',id,'title',title,'state',CASE WHEN state='executing' AND updated<floor(extract(epoch FROM clock_timestamp()))::bigint-60 THEN 'uncertain' ELSE state END) AS data FROM relay_control_actions WHERE workspace=$1 AND job=$2 AND requires_approval ORDER BY created DESC,id DESC LIMIT 5",vec![w.into(),job.reply.clone().into()]).await?
    } else {
        vec![]
    };
    let can_cancel = job_state != "finished"
        && message_state == "queued"
        && (job.requester == user.id || role(tx, w, &user.id).await? == "Owner");
    let cancellation=query(tx,"SELECT COALESCE(u.display_name,u.email,'Former member') AS name,a.at FROM relay_audit a LEFT JOIN relay_users u ON u.id=a.actor WHERE a.workspace=$1 AND a.action='agent_request_cancelled' AND a.target=$2 ORDER BY a.id DESC LIMIT 1",vec![w.into(),job.reply.clone().into()]).await?.map(|r|->Result<Value>{Ok(json!({"by":r.try_get::<String>("","name")?,"at":r.try_get::<i64>("","at")?}))}).transpose()?;
    Ok(Some(
        json!({"source_scope":super::event_access::scope(tx,w,&job.reply).await?,"id":job.reply,"sequence":row.try_get::<i64>("","sequence")?,"chat":job.chat,"thread":row.try_get::<Option<String>>("","thread")?,"source_message":row.try_get::<Option<String>>("","source_message")?,"conversation_name":row.try_get::<String>("","conversation_name")?,"requester":job.requester,"requester_name":row.try_get::<Option<String>>("","requester_name")?,"summary":row.try_get::<Option<String>>("","summary")?.unwrap_or_else(||"Agent request".into()),"state":state,"latest_update":latest.chars().take(1000).collect::<String>(),"created_ms":row.try_get::<i64>("","created_ms")?,"started_ms":row.try_get::<Option<i64>>("","started_ms")?,"finished_ms":row.try_get::<Option<i64>>("","finished_ms")?,"can_cancel":can_cancel,"cancellation":cancellation,"approvals":approvals}),
    ))
}

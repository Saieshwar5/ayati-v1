use super::*;
use crate::identity::chat::model::Job;
impl PgStore {
    pub(crate) async fn response_body(
        &self,
        user: &User,
        w: &str,
        chat: &str,
        id: &str,
        metadata: bool,
    ) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        super::chat::access(&tx, user, w, chat).await?;
        let row=query(&tx,"SELECT m.content,m.state,m.progress,m.revision,j.payload FROM relay_chat_messages m LEFT JOIN relay_chat_jobs j ON j.workspace=m.workspace AND j.reply=m.id WHERE m.workspace=$1 AND m.chat=$2 AND m.id=$3 AND m.kind='agent'",vec![w.into(),chat.into(),id.into()]).await?.context("Saved response unavailable")?;
        let state: String = row.try_get("", "state")?;
        if let Some(payload) = row.try_get::<Option<Value>>("", "payload")? {
            let job: Job = serde_json::from_value(payload)?;
            ensure!(
                job.workspace == w && job.chat == chat && job.reply == id,
                "Response scope mismatch"
            );
            let mut viewer = job.clone();
            viewer.requester = user.id.clone();
            ensure!(
                super::source_access::allowed(&tx, &viewer).await?
                    && super::context_attempts::allowed(&tx, &viewer).await?,
                "Response source unavailable"
            );
            if state == "queued" {
                super::controlled_attempts::access(&tx, &job).await?;
            }
        }
        if row.try_get::<Option<Value>>("", "payload")?.is_none()
            && let Some(mut viewer) = super::handoff_access::message_job(&tx, w, id).await?
        {
            ensure!(
                viewer.chat == chat && viewer.workspace == w,
                "Handoff message scope mismatch"
            );
            viewer.requester = user.id.clone();
            ensure!(
                super::source_access::allowed(&tx, &viewer).await?
                    && super::context_attempts::allowed(&tx, &viewer).await?,
                "Handoff message source unavailable"
            );
        }
        let content: String = row.try_get("", "content")?;
        let progress: Option<Value> = row.try_get("", "progress")?;
        let partial = if !matches!(state.as_str(), "queued" | "complete" | "sent") {
            partial_output(&tx, w, id).await?
        } else {
            None
        };
        let text = partial
            .as_deref()
            .or_else(|| {
                progress
                    .as_ref()
                    .and_then(|v| v["text"].as_str())
                    .filter(|s| !s.is_empty())
            })
            .unwrap_or(&content);
        let mut data =
            crate::response::body(id, text, &state, row.try_get("", "revision")?, metadata)?;
        data["source_scope"] = serde_json::json!(super::event_access::scope(&tx, w, id).await?);
        tx.commit().await?;
        Ok(data)
    }
}
/// What a stopped reply wrote: the text saved when its chunks were compacted,
/// or else the latest attempt's streamed chunks.
pub(super) async fn partial_output(
    tx: &DatabaseTransaction,
    w: &str,
    id: &str,
) -> Result<Option<String>> {
    if let Some(row) = query(
        tx,
        "SELECT text FROM relay_reply_partials WHERE workspace=$1 AND reply=$2",
        vec![w.into(), id.into()],
    )
    .await?
    {
        return Ok(Some(row.try_get::<String>("", "text")?).filter(|t| !t.is_empty()));
    }
    let row = query(tx,"SELECT string_agg(o.text,'' ORDER BY o.sequence) AS text FROM relay_assignment_output o WHERE o.assignment=(SELECT a.id FROM relay_worker_assignments a WHERE a.workspace=$1 AND a.reply=$2 AND EXISTS(SELECT 1 FROM relay_assignment_output present WHERE present.assignment=a.id AND present.text<>'') ORDER BY a.generation DESC LIMIT 1)",vec![w.into(),id.into()]).await?.context("Missing output aggregate")?;
    Ok(row
        .try_get::<Option<String>>("", "text")?
        .filter(|text| !text.is_empty()))
}

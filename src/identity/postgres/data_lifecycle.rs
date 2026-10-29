//! Removes duplicates and delivery notices, never history (D226). Each batch is
//! one short transaction that first takes a cleanup lock without waiting, so
//! with several controllers only one works at a time and the rest skip.
use super::*;

async fn claimed(tx: &DatabaseTransaction) -> Result<bool> {
    Ok(query(tx,"SELECT pg_try_advisory_xact_lock(hashtextextended(current_database() || '/' || current_schema() || '/relay_data_lifecycle',0)) AS locked",vec![])
        .await?
        .context("Cleanup lock unavailable")?
        .try_get("", "locked")?)
}

impl PgStore {
    /// For replies that stopped at least `settle_seconds` ago, save the partial
    /// text a reader would see, then remove their streamed chunks: the message
    /// keeps the text. Returns how many replies were compacted.
    pub(crate) async fn compact_replies(&self, settle_seconds: i64, limit: i64) -> Result<u64> {
        let tx = self.db.begin().await?;
        if !claimed(&tx).await? {
            return Ok(0);
        }
        let replies = rows(&tx,"SELECT DISTINCT a.workspace,a.reply,m.state FROM relay_worker_assignments a
            JOIN relay_chat_messages m ON m.workspace=a.workspace AND m.id=a.reply AND m.state<>'queued'
            WHERE a.reply IS NOT NULL AND a.state IN ('finished','lost')
            AND a.finished_ms<(extract(epoch FROM clock_timestamp())*1000)::bigint-$1*1000
            AND EXISTS(SELECT 1 FROM relay_assignment_output o WHERE o.assignment=a.id)
            LIMIT $2",vec![settle_seconds.into(),limit.into()]).await?;
        for row in &replies {
            let (w, reply): (String, String) =
                (row.try_get("", "workspace")?, row.try_get("", "reply")?);
            let state: String = row.try_get("", "state")?;
            if !matches!(state.as_str(), "complete" | "sent")
                && let Some(text) = super::responses::partial_output(&tx, &w, &reply).await?
            {
                execute(&tx,"INSERT INTO relay_reply_partials(workspace,reply,text) VALUES($1,$2,$3) ON CONFLICT DO NOTHING",
                    vec![w.clone().into(),reply.clone().into(),text.into()]).await?;
            }
            execute(&tx,"DELETE FROM relay_assignment_output o USING relay_worker_assignments a WHERE o.assignment=a.id
                AND a.workspace=$1 AND a.reply=$2 AND a.state IN ('finished','lost')",vec![w.into(),reply.into()]).await?;
        }
        tx.commit().await?;
        Ok(replies.len() as u64)
    }

    /// Remove live-update notices older than `days` and raise each workspace's
    /// floor, so an older browser position reloads instead of missing changes.
    pub(crate) async fn expire_sync_events(&self, days: i64, limit: i64) -> Result<u64> {
        let tx = self.db.begin().await?;
        if !claimed(&tx).await? {
            return Ok(0);
        }
        let removed: i64 = query(&tx,"WITH old AS (
            SELECT workspace,cursor FROM relay_sync_events
            WHERE created_ms<(extract(epoch FROM clock_timestamp())*1000)::bigint-$1*86400000
            ORDER BY created_ms LIMIT $2
          ), gone AS (
            DELETE FROM relay_sync_events e USING old WHERE e.workspace=old.workspace AND e.cursor=old.cursor
            RETURNING e.workspace,e.cursor
          ), raised AS (
            UPDATE relay_sync_clocks c SET floor=GREATEST(c.floor,top.cursor)
            FROM (SELECT workspace,max(cursor) AS cursor FROM gone GROUP BY workspace) top
            WHERE c.workspace=top.workspace RETURNING c.workspace
          ) SELECT (SELECT count(*) FROM gone) AS removed",vec![days.into(),limit.into()])
            .await?
            .context("Cleanup result unavailable")?
            .try_get("", "removed")?;
        tx.commit().await?;
        Ok(removed as u64)
    }

    /// Finished attempts older than `days`, oldest first, as archive records.
    /// Secrets and context are excluded: token hashes, session, completion
    /// fingerprint and request identity stay out of the archive.
    pub(crate) async fn stale_attempts(&self, days: i64, limit: i64) -> Result<Vec<Value>> {
        rows(&self.db,"SELECT to_jsonb(a) || jsonb_build_object('executor',c.executor,'incarnation',c.incarnation,
              'slot',c.slot,'slot_generation',c.slot_generation,'completion_state',c.completion_state,
              'archive_day',to_char(to_timestamp(a.finished_ms/1000) AT TIME ZONE 'UTC','YYYY/MM/DD')) AS record
            FROM relay_worker_assignments a LEFT JOIN relay_assignment_credentials c ON c.assignment=a.id
            WHERE a.state IN ('finished','lost')
            AND a.finished_ms<(extract(epoch FROM clock_timestamp())*1000)::bigint-$1*86400000
            ORDER BY a.finished_ms,a.id LIMIT $2",vec![days.into(),limit.into()]).await?
            .into_iter()
            .map(|row| Ok(row.try_get("", "record")?))
            .collect()
    }

    /// Remove archived attempts and their dependent rows. A dead executor's
    /// idle slot may still point at an old attempt; that pointer is cleared.
    pub(crate) async fn remove_attempts(&self, ids: &[String]) -> Result<u64> {
        let tx = self.db.begin().await?;
        if !claimed(&tx).await? {
            return Ok(0);
        }
        let ids = Value::from(ids.to_vec());
        let set = "SELECT jsonb_array_elements_text($1::jsonb)";
        execute(&tx,&format!("UPDATE relay_executor_slots SET assignment=NULL WHERE assignment IN ({set}) AND state NOT IN ('offered','running')"),vec![ids.clone().into()]).await?;
        let removable = format!("SELECT a.id FROM relay_worker_assignments a WHERE a.id IN ({set}) AND a.state IN ('finished','lost')
            AND NOT EXISTS(SELECT 1 FROM relay_executor_slots s WHERE s.assignment=a.id)");
        execute(
            &tx,
            &format!("DELETE FROM relay_assignment_output WHERE assignment IN ({removable})"),
            vec![ids.clone().into()],
        )
        .await?;
        execute(
            &tx,
            &format!("DELETE FROM relay_assignment_credentials WHERE assignment IN ({removable})"),
            vec![ids.clone().into()],
        )
        .await?;
        let removed = execute(
            &tx,
            &format!("DELETE FROM relay_worker_assignments WHERE id IN ({removable})"),
            vec![ids.into()],
        )
        .await?;
        tx.commit().await?;
        Ok(removed)
    }
}

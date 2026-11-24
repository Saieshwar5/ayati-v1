//! The nightly tidy-up of a workspace's memory, on every shelf, never mixing
//! shelves: notes past their date end, the same note kept twice becomes one,
//! notes nobody used fade into the archive, the archive is emptied after a
//! year, and topics with many or old notes get a summary written by a model.
//! Each change is recorded with the notes as they were, so a person can undo it.
use super::*;
use serde_json::json;

/// At most one tidy-up a day per workspace.
const EVERY_MS: i64 = 20 * 3_600_000;
/// Archived and ended notes are deleted after this long; changes are kept for undo this long.
const KEEP_MS: i64 = 365 * 86_400_000;
const CHANGES_KEPT_MS: i64 = 90 * 86_400_000;
/// A topic gets a summary from this many notes, and its notes older than
/// `FOLD_AFTER_MS` move into it.
const SUMMARY_FROM: i64 = 25;
const FOLD_AFTER_MS: i64 = 90 * 86_400_000;

/// One topic to summarise: its notes, and those that move into the summary.
pub(crate) struct TopicWork {
    pub workspace: String,
    pub shelf: String,
    pub topic: String,
    pub summary: Option<String>,
    pub notes: Vec<String>,
    pub fold: Vec<String>,
}

/// Records one change of `action` per shelf for the rows `hit` selects
/// (`$1` workspace, `$2` now), then applies `then`.
async fn logged(
    tx: &DatabaseTransaction,
    w: &str,
    now_ms: i64,
    action: &str,
    hit: &str,
    summary: &str,
    then: &str,
) -> Result<u64> {
    execute(
        tx,
        &format!("WITH hit AS ({hit})
         INSERT INTO relay_memory_changes(workspace,id,scope,agent,user_id,shelf,action,summary,before,at_ms)
         SELECT $1,gen_random_uuid()::text,min(scope),min(agent),min(user_id),shelf,'{action}',{summary},
           jsonb_build_object('memories',jsonb_agg(to_jsonb(hit)-'shelf')),$2 FROM hit GROUP BY shelf"),
        vec![w.into(), now_ms.into()],
    )
    .await?;
    execute(tx, then, vec![w.into(), now_ms.into()]).await
}

impl PgStore {
    /// Workspaces with memory whose tidy-up is due.
    pub(crate) async fn memory_tidy_due(&self, now_ms: i64, limit: i64) -> Result<Vec<String>> {
        if !ready(&self.db).await? {
            return Ok(vec![]);
        }
        rows(
            &self.db,
            "SELECT w.id FROM relay_workspaces w LEFT JOIN relay_memory_tidy t ON t.workspace=w.id
             WHERE (t.last_run_ms IS NULL OR t.last_run_ms<$1-$2) AND EXISTS(SELECT 1 FROM relay_memories m WHERE m.workspace=w.id)
             ORDER BY t.last_run_ms NULLS FIRST LIMIT $3",
            vec![now_ms.into(), EVERY_MS.into(), limit.into()],
        )
        .await?
        .into_iter()
        .map(|r| Ok(r.try_get("", "id")?))
        .collect()
    }

    /// The tidy-up's steps that need no model. Returns what each changed.
    pub(crate) async fn tidy_memory(&self, w: &str, now_ms: i64) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        execute(
            &tx,
            "INSERT INTO relay_memory_tidy(workspace,last_run_ms) VALUES($1,$2) ON CONFLICT(workspace) DO UPDATE SET last_run_ms=EXCLUDED.last_run_ms",
            vec![w.into(), now_ms.into()],
        )
        .await?;
        // Notes past their date end.
        let expired = logged(&tx, w, now_ms, "expired",
            "SELECT * FROM relay_memories WHERE workspace=$1 AND state='active' AND until_ms IS NOT NULL AND until_ms<$2",
            "CASE count(*) WHEN 1 THEN 'Ended a note whose date passed: '||min(text) ELSE 'Ended '||count(*)||' notes whose date passed' END",
            "UPDATE relay_memories SET state='ended',changed_ms=$2 WHERE workspace=$1 AND state='active' AND until_ms IS NOT NULL AND until_ms<$2").await?;
        // The same note kept twice on a shelf becomes one, as strong as both.
        let groups = rows(
            &tx,
            "SELECT shelf,btrim(regexp_replace(lower(text),'[^[:alnum:]]+',' ','g')) AS norm FROM relay_memories
             WHERE workspace=$1 AND state='active' GROUP BY 1,2 HAVING count(*)>1 LIMIT 200",
            vec![w.into()],
        )
        .await?;
        let mut merged = 0;
        for group in groups {
            let shelf: String = group.try_get("", "shelf")?;
            let norm: String = group.try_get("", "norm")?;
            let found = rows(
                &tx,
                "SELECT id,to_jsonb(m)-'shelf' AS row FROM relay_memories m WHERE workspace=$1 AND shelf=$2 AND state='active'
                   AND btrim(regexp_replace(lower(text),'[^[:alnum:]]+',' ','g'))=$3 ORDER BY pinned DESC,created_ms,id",
                vec![w.into(), shelf.clone().into(), norm.into()],
            )
            .await?;
            let ids: Vec<String> = found
                .iter()
                .map(|r| r.try_get("", "id"))
                .collect::<Result<_, _>>()?;
            let before: Vec<Value> = found
                .iter()
                .map(|r| r.try_get("", "row"))
                .collect::<Result<_, _>>()?;
            let (kept, rest) = ids.split_first().context("Empty merge")?;
            let first = &before[0];
            execute(
                &tx,
                "INSERT INTO relay_memory_changes(workspace,id,scope,agent,user_id,shelf,action,summary,before,at_ms)
                 VALUES($1,gen_random_uuid()::text,$2,$3,$4,$5,'merged',$6,$7,$8)",
                vec![w.into(), first["scope"].as_str().unwrap_or_default().into(), first["agent"].as_str().map(str::to_owned).into(),
                    first["user_id"].as_str().map(str::to_owned).into(), shelf.into(),
                    format!("Merged {} copies of: {}", ids.len(), first["text"].as_str().unwrap_or_default()).into(),
                    json!({"memories":before}).into(), now_ms.into()],
            )
            .await?;
            execute(
                &tx,
                "UPDATE relay_memories k SET strength=least(100,s.strength+15),used=s.used,used_ms=s.used_ms,changed_ms=$3
                 FROM (SELECT max(strength) AS strength,sum(used)::integer AS used,max(used_ms) AS used_ms FROM relay_memories
                   WHERE workspace=$1 AND id=ANY(string_to_array($4,','))) s WHERE k.workspace=$1 AND k.id=$2",
                vec![w.into(), kept.clone().into(), now_ms.into(), ids.join(",").into()],
            )
            .await?;
            execute(
                &tx,
                "UPDATE relay_memories SET state='ended',replaced_by=$2,changed_ms=$3 WHERE workspace=$1 AND id=ANY(string_to_array($4,','))",
                vec![w.into(), kept.clone().into(), now_ms.into(), rest.join(",").into()],
            )
            .await?;
            merged += rest.len();
        }
        // Notes nobody used fade; below the line they go to the archive.
        let fade = format!(
            "state='active' AND NOT pinned AND relay_memory_strength(strength,kind,used_ms,pinned,$2)<{ARCHIVE_BELOW}"
        );
        let archived = logged(&tx, w, now_ms, "archived",
            &format!("SELECT * FROM relay_memories WHERE workspace=$1 AND {fade}"),
            "CASE count(*) WHEN 1 THEN 'Archived a note nobody used: '||min(text) ELSE 'Archived '||count(*)||' notes nobody used' END",
            &format!("UPDATE relay_memories SET state='archived',changed_ms=$2 WHERE workspace=$1 AND {fade}")).await?;
        // The archive and ended notes are emptied after a year.
        let old = format!(
            "state IN ('archived','ended') AND COALESCE(changed_ms,created_ms)<$2-{KEEP_MS}"
        );
        let deleted = logged(
            &tx,
            w,
            now_ms,
            "deleted",
            &format!("SELECT * FROM relay_memories WHERE workspace=$1 AND {old}"),
            "'Deleted '||count(*)||' notes after a year unused'",
            &format!("DELETE FROM relay_memories WHERE workspace=$1 AND {old}"),
        )
        .await?;
        execute(
            &tx,
            &format!(
                "DELETE FROM relay_memory_changes WHERE workspace=$1 AND at_ms<$2-{CHANGES_KEPT_MS}"
            ),
            vec![w.into(), now_ms.into()],
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"expired":expired,"merged":merged,"archived":archived,"deleted":deleted}))
    }

    /// Topics due a summary: many notes, or notes old enough to fold in.
    pub(crate) async fn memory_topics_to_summarise(
        &self,
        w: &str,
        now_ms: i64,
        limit: i64,
    ) -> Result<Vec<TopicWork>> {
        let due = rows(
            &self.db,
            &format!("SELECT m.shelf,m.topic,(SELECT t.summary FROM relay_memory_topics t WHERE t.workspace=m.workspace AND t.shelf=m.shelf AND t.topic=m.topic) AS summary
             FROM relay_memories m WHERE m.workspace=$1 AND m.state='active' AND m.topic<>'' GROUP BY m.workspace,m.shelf,m.topic
             HAVING count(*)>={SUMMARY_FROM} OR bool_or(NOT m.pinned AND m.created_ms<$2-{FOLD_AFTER_MS})
             ORDER BY count(*) DESC LIMIT $3"),
            vec![w.into(), now_ms.into(), limit.into()],
        )
        .await?;
        let mut work = vec![];
        for row in due {
            let shelf: String = row.try_get("", "shelf")?;
            let topic: String = row.try_get("", "topic")?;
            let notes = rows(
                &self.db,
                "SELECT id,text,pinned,created_ms FROM relay_memories WHERE workspace=$1 AND shelf=$2 AND topic=$3 AND state='active'
                 ORDER BY pinned DESC,strength DESC,created_ms DESC LIMIT 60",
                vec![w.into(), shelf.clone().into(), topic.clone().into()],
            )
            .await?;
            let mut texts = vec![];
            let mut fold = vec![];
            for note in notes {
                texts.push(note.try_get::<String>("", "text")?);
                if !note.try_get::<bool>("", "pinned")?
                    && note.try_get::<i64>("", "created_ms")? < now_ms - FOLD_AFTER_MS
                {
                    fold.push(note.try_get("", "id")?);
                }
            }
            work.push(TopicWork {
                workspace: w.into(),
                shelf,
                topic,
                summary: row.try_get("", "summary")?,
                notes: texts,
                fold,
            });
        }
        Ok(work)
    }

    /// Saves a topic's new summary and folds its old notes into it, recording the change.
    pub(crate) async fn save_memory_summary(
        &self,
        work: &TopicWork,
        summary: &str,
        now_ms: i64,
    ) -> Result<()> {
        let summary: String = summary.trim().chars().take(1200).collect();
        ensure!(!summary.is_empty(), "Empty summary");
        let w = &work.workspace;
        let tx = self.workspace_tx(w).await?;
        let topic = json_rows(&tx, "SELECT to_jsonb(t)-'shelf' AS data FROM relay_memory_topics t WHERE workspace=$1 AND shelf=$2 AND topic=$3",
            vec![w.clone().into(), work.shelf.clone().into(), work.topic.clone().into()]).await?.into_iter().next();
        let folded = json_rows(&tx, "SELECT to_jsonb(m)-'shelf' AS data FROM relay_memories m WHERE workspace=$1 AND id=ANY(string_to_array($2,',')) AND state='active'",
            vec![w.clone().into(), work.fold.join(",").into()]).await?;
        let parts: Vec<&str> = work.shelf.splitn(4, ':').collect();
        let [scope, agent, chat, user] = parts[..] else {
            anyhow::bail!("Invalid shelf");
        };
        let some = |v: &str| (!v.is_empty()).then(|| v.to_owned());
        execute(
            &tx,
            "INSERT INTO relay_memory_changes(workspace,id,scope,agent,user_id,shelf,action,summary,before,at_ms)
             VALUES($1,gen_random_uuid()::text,$2,$3,$4,$5,'summarised',$6,$7,$8)",
            vec![w.clone().into(), scope.into(), some(agent).into(), some(user).into(), work.shelf.clone().into(),
                format!("Wrote the summary of {} from {} notes", work.topic, work.notes.len()).into(),
                json!({"memories":folded,"topic":topic,"workspace":w,"shelf":work.shelf,"topic_name":work.topic}).into(), now_ms.into()],
        )
        .await?;
        execute(
            &tx,
            "INSERT INTO relay_memory_topics(workspace,scope,agent,chat,user_id,topic,summary,source,updated_ms) VALUES($1,$2,$3,$4,$5,$6,$7,'tidy',$8)
             ON CONFLICT(workspace,shelf,topic) DO UPDATE SET summary=EXCLUDED.summary,source='tidy',updated_ms=EXCLUDED.updated_ms",
            vec![w.clone().into(), scope.into(), some(agent).into(), some(chat).into(), some(user).into(), work.topic.clone().into(), summary.into(), now_ms.into()],
        )
        .await?;
        execute(
            &tx,
            "UPDATE relay_memories SET state='folded',changed_ms=$3 WHERE workspace=$1 AND id=ANY(string_to_array($2,',')) AND state='active' AND NOT pinned",
            vec![w.clone().into(), work.fold.join(",").into(), now_ms.into()],
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

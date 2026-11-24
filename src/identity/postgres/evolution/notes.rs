//! Notes after they are kept: checked once by the classifier (kind, one-off,
//! private, an instruction, and how it relates to the nearest notes on its shelf),
//! given a meaning (embedding) where pgvector is installed, and recorded on each
//! request that was given or found them.
use super::super::memory::{NOW_MS, STRENGTH};
use super::*;
use serde_json::json;

/// A note to check, with the notes nearest to it on the same shelf.
#[derive(Debug, Clone)]
pub(crate) struct NoteCheck {
    pub workspace: String,
    pub id: String,
    pub scope: String,
    pub kind: String,
    pub text: String,
    pub strength: i32,
    /// (id, text, strength now, pinned)
    pub nearest: Vec<(String, String, i32, bool)>,
}

/// What the classifier said about one note.
#[derive(Debug, Default)]
pub(crate) struct Verdict {
    pub kind: Option<String>,
    pub one_off: bool,
    pub private: bool,
    pub instruction: bool,
    /// (nearest note id, "same" or "contradicts", confidence)
    pub relations: Vec<(String, String, f64)>,
    pub model: String,
}

/// The schema pgvector is installed in, when notes have meanings.
pub(crate) async fn vectors(db: &impl ConnectionTrait) -> Result<Option<String>> {
    Ok(query(db, "SELECT n.nspname AS schema FROM pg_extension e JOIN pg_namespace n ON n.oid=e.extnamespace WHERE e.extname='vector' AND to_regclass('relay_memory_embeddings') IS NOT NULL", vec![])
        .await?.map(|r| r.try_get("", "schema")).transpose()?)
}

/// Records the notes a request was given or found, for its outcome.
pub(crate) async fn record_used(
    db: &impl ConnectionTrait,
    w: &str,
    job: &str,
    ids: &[String],
    how: &str,
) -> Result<()> {
    if ids.is_empty() || !ready(db).await? {
        return Ok(());
    }
    execute(
        db,
        "INSERT INTO relay_memory_used(workspace,job,memory,how) SELECT $1,$2,id,$4 FROM unnest(string_to_array($3,',')) AS id
         WHERE EXISTS(SELECT 1 FROM relay_memories m WHERE m.workspace=$1 AND m.id=id) ON CONFLICT DO NOTHING",
        vec![w.into(), job.into(), ids.join(",").into(), how.into()],
    )
    .await?;
    Ok(())
}

impl PgStore {
    /// Notes agents kept that the classifier has not checked, oldest first.
    pub(crate) async fn notes_to_check(&self, limit: i64) -> Result<Vec<NoteCheck>> {
        if !ready(&self.db).await? {
            return Ok(vec![]);
        }
        let found = rows(&self.db, &format!("SELECT m.workspace,m.id,m.scope,m.kind,m.text,m.shelf,{STRENGTH} AS strength FROM relay_memories m
            WHERE m.checked_ms IS NULL AND m.source='agent' AND m.state IN ('active','waiting') ORDER BY m.created_ms LIMIT $1"), vec![limit.into()]).await?;
        let mut checks = vec![];
        for row in found {
            let (w, id, shelf): (String, String, String) = (
                row.try_get("", "workspace")?,
                row.try_get("", "id")?,
                row.try_get("", "shelf")?,
            );
            let text: String = row.try_get("", "text")?;
            let words = super::super::memory::any_words(&text, 12).unwrap_or_default();
            let nearest = rows(&self.db, &format!("SELECT m.id,m.text,{STRENGTH} AS strength,m.pinned FROM relay_memories m
                WHERE m.workspace=$1 AND m.shelf=$2 AND m.id<>$3 AND m.state='active' AND $4<>''
                  AND to_tsvector('simple',m.topic||' '||m.text) @@ to_tsquery('simple',$4)
                ORDER BY ts_rank(to_tsvector('simple',m.topic||' '||m.text),to_tsquery('simple',$4)) DESC LIMIT 5"),
                vec![w.clone().into(), shelf.into(), id.clone().into(), words.into()]).await?
                .into_iter()
                .map(|r| Ok((r.try_get("", "id")?, r.try_get("", "text")?, r.try_get("", "strength")?, r.try_get("", "pinned")?)))
                .collect::<Result<Vec<_>>>()?;
            checks.push(NoteCheck {
                workspace: w,
                id,
                scope: row.try_get("", "scope")?,
                kind: row.try_get("", "kind")?,
                text,
                strength: row.try_get("", "strength")?,
                nearest,
            });
        }
        Ok(checks)
    }

    /// Applies what the classifier said about a note, once. Returns what changed.
    pub(crate) async fn save_note_check(
        &self,
        note: &NoteCheck,
        verdict: &Verdict,
    ) -> Result<&'static str> {
        let w = &note.workspace;
        let tx = self.workspace_tx(w).await?;
        let result = json!({"kind":verdict.kind,"one_off":verdict.one_off,"private":verdict.private,"instruction":verdict.instruction,
            "relations":verdict.relations.iter().map(|(id, r, c)| json!({"id":id,"relation":r,"confidence":c})).collect::<Vec<_>>(),"model":verdict.model});
        let marked = execute(&tx, &format!("UPDATE relay_memories SET checked_ms={NOW_MS},check_result=$3 WHERE workspace=$1 AND id=$2 AND checked_ms IS NULL"),
            vec![w.clone().into(), note.id.clone().into(), result.into()]).await?;
        if marked == 0 {
            return Ok("already");
        }
        let set = |sql: &str| {
            format!(
                "UPDATE relay_memories SET {sql},changed_ms={NOW_MS} WHERE workspace=$1 AND id=$2"
            )
        };
        let args = || vec![w.clone().into(), note.id.clone().into()];
        let mut done = "checked";
        if verdict.instruction || verdict.one_off {
            // Notes are lasting facts, never orders or one-off details.
            execute(&tx, &set("state='ended'"), args()).await?;
            done = if verdict.instruction {
                "ended_instruction"
            } else {
                "ended_one_off"
            };
        } else if verdict.private && ["job", "workspace"].contains(&note.scope.as_str()) {
            execute(
                &tx,
                &set("state='waiting',waiting_reason='private_source'"),
                args(),
            )
            .await?;
            done = "waiting_private";
        } else {
            if let Some(kind) = verdict
                .kind
                .as_deref()
                .filter(|k| *k != note.kind && super::super::memory::KINDS.contains(k))
            {
                execute(
                    &tx,
                    "UPDATE relay_memories SET kind=$3 WHERE workspace=$1 AND id=$2",
                    vec![w.clone().into(), note.id.clone().into(), kind.into()],
                )
                .await?;
            }
            for (other, relation, confidence) in &verdict.relations {
                let Some((_, _, strength, pinned)) = note.nearest.iter().find(|n| &n.0 == other)
                else {
                    continue;
                };
                match relation.as_str() {
                    "same" if *confidence >= ACT => {
                        // The older note stays, as strong as both.
                        execute(&tx, &format!("UPDATE relay_memories SET strength=least(100,strength+15),used_ms={NOW_MS},changed_ms={NOW_MS} WHERE workspace=$1 AND id=$2"),
                            vec![w.clone().into(), other.clone().into()]).await?;
                        execute(
                            &tx,
                            &set("state='ended',replaced_by=$3"),
                            vec![
                                w.clone().into(),
                                note.id.clone().into(),
                                other.clone().into(),
                            ],
                        )
                        .await?;
                        done = "merged";
                    }
                    "contradicts"
                        if *confidence >= ACT && !pinned && *strength <= note.strength + 10 =>
                    {
                        execute(&tx, &format!("UPDATE relay_memories SET state='ended',replaced_by=$3,changed_ms={NOW_MS} WHERE workspace=$1 AND id=$2 AND state='active'"),
                            vec![w.clone().into(), other.clone().into(), note.id.clone().into()]).await?;
                        execute(
                            &tx,
                            &set("replaces=$3"),
                            vec![
                                w.clone().into(),
                                note.id.clone().into(),
                                other.clone().into(),
                            ],
                        )
                        .await?;
                        done = "replaced";
                    }
                    "contradicts" if *confidence >= ASK => {
                        execute(
                            &tx,
                            &set("state='waiting',waiting_reason='disagreement',replaces=$3"),
                            vec![
                                w.clone().into(),
                                note.id.clone().into(),
                                other.clone().into(),
                            ],
                        )
                        .await?;
                        done = "waiting_disagreement";
                    }
                    _ => continue,
                }
                break;
            }
        }
        tx.commit().await?;
        Ok(done)
    }

    /// The agent that kept a note, for recording the model use its check cost.
    pub(crate) async fn memory_author(&self, w: &str, id: &str) -> Result<Option<String>> {
        Ok(query(&self.db, "SELECT m.author FROM relay_memories m JOIN relay_shared_agents a ON a.workspace=m.workspace AND a.id=m.author WHERE m.workspace=$1 AND m.id=$2",
            vec![w.into(), id.into()]).await?.map(|r| r.try_get("", "author")).transpose()?)
    }

    /// Notes with no meaning from `model` yet, where pgvector is installed.
    pub(crate) async fn notes_to_embed(
        &self,
        model: &str,
        limit: i64,
    ) -> Result<Vec<(String, String, String)>> {
        if vectors(&self.db).await?.is_none() {
            return Ok(vec![]);
        }
        rows(&self.db, "SELECT m.workspace,m.id,m.topic||' '||m.text AS text FROM relay_memories m
            LEFT JOIN relay_memory_embeddings e ON e.workspace=m.workspace AND e.memory=m.id AND e.model=$1
            WHERE e.memory IS NULL AND m.state IN ('active','waiting') ORDER BY m.created_ms DESC LIMIT $2", vec![model.into(), limit.into()])
            .await?
            .into_iter()
            .map(|r| Ok((r.try_get("", "workspace")?, r.try_get("", "id")?, r.try_get("", "text")?)))
            .collect()
    }

    /// Stores notes' meanings: (workspace, note, vector as text).
    pub(crate) async fn store_note_embeddings(
        &self,
        model: &str,
        embedded: &[(String, String, String)],
    ) -> Result<()> {
        let Some(schema) = vectors(&self.db).await? else {
            return Ok(());
        };
        let items: Vec<Value> = embedded
            .iter()
            .map(|(w, id, v)| json!({"w":w,"id":id,"v":v}))
            .collect();
        execute(&self.db, &format!("INSERT INTO relay_memory_embeddings(workspace,memory,model,embedding)
            SELECT e.r->>'w',e.r->>'id',$1,(e.r->>'v')::{schema}.vector FROM jsonb_array_elements($2::jsonb) AS e(r)
            JOIN relay_memories m ON m.workspace=e.r->>'w' AND m.id=e.r->>'id'
            ON CONFLICT(workspace,memory) DO UPDATE SET model=EXCLUDED.model,embedding=EXCLUDED.embedding"), vec![model.into(), json!(items).into()]).await?;
        Ok(())
    }
}

/// Notes on `shelves` nearest in meaning to `vector`: (id, distance).
pub(crate) async fn nearest_in_meaning(
    db: &impl ConnectionTrait,
    w: &str,
    filter: &str,
    args: Vec<sea_orm::Value>,
    vector: &str,
    model: &str,
) -> Result<Vec<(String, f64)>> {
    let Some(schema) = vectors(db).await? else {
        return Ok(vec![]);
    };
    let n = args.len();
    let mut all: Vec<sea_orm::Value> = vec![w.into()];
    all.extend(args);
    all.push(vector.into());
    all.push(model.into());
    rows(db, &format!("SELECT m.id,(e.embedding OPERATOR({schema}.<=>) ${v}::{schema}.vector)::float8 AS distance
        FROM relay_memory_embeddings e JOIN relay_memories m ON m.workspace=e.workspace AND m.id=e.memory
        WHERE e.workspace=$1 AND e.model=${md} AND {filter} AND {active}
        ORDER BY distance LIMIT 15", v = n + 2, md = n + 3, active = super::super::memory::ACTIVE), all)
        .await?
        .into_iter()
        .map(|r| Ok((r.try_get("", "id")?, r.try_get("", "distance")?)))
        .collect()
}

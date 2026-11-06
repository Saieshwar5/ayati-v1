//! Library meaning search (LB2b): embeddings of each current passage and a one-line
//! summary per document, written by the controller's indexer, and the passages
//! nearest a question's meaning. Without pgvector only summaries are kept.
use super::*;
use crate::identity::chat::model::Job;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};

/// Schema 72: summaries, and passage embeddings where pgvector is installed.
pub(crate) struct LibraryMeaning;
impl MigrationName for LibraryMeaning {
    fn name(&self) -> &str {
        "m20261105_000072_library_meaning"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for LibraryMeaning {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("meaning.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Library meaning search requires a forward migration".into(),
        ))
    }
}

const BATCH: i64 = 64;

/// The schema pgvector is installed in, when this database stores embeddings.
pub(super) async fn vectors(db: &impl ConnectionTrait) -> Result<Option<String>> {
    Ok(query(db,"SELECT n.nspname AS schema FROM pg_extension e JOIN pg_namespace n ON n.oid=e.extnamespace WHERE e.extname='vector' AND to_regclass('relay_library_embeddings') IS NOT NULL",vec![]).await?.map(|r| r.try_get("", "schema")).transpose()?)
}

/// One workspace's passages waiting for an embedding from this model.
pub(crate) struct Pending {
    pub workspace: String,
    pub passages: Vec<(String, i32, i64, String)>,
}

impl PgStore {
    pub(crate) async fn library_pending_embeddings(&self, model: &str) -> Result<Option<Pending>> {
        if vectors(&self.db).await?.is_none() {
            return Ok(None);
        }
        const WAITING: &str = "FROM relay_library_passages p JOIN relay_library_items i ON i.workspace=p.workspace AND i.id=p.item AND NOT i.trashed AND p.revision=i.revision LEFT JOIN relay_library_embeddings e ON e.workspace=p.workspace AND e.item=p.item AND e.position=p.position AND e.model=$1 AND e.revision=p.revision WHERE e.item IS NULL";
        let Some(row) = query(
            &self.db,
            &format!("SELECT p.workspace {WAITING} LIMIT 1"),
            vec![model.into()],
        )
        .await?
        else {
            return Ok(None);
        };
        let workspace: String = row.try_get("", "workspace")?;
        let passages = rows(&self.db,&format!("SELECT p.item,p.position,p.revision,p.title || CASE WHEN p.section='' THEN '' ELSE ' › ' || p.section END || E'\\n' || p.text AS text {WAITING} AND p.workspace=$2 ORDER BY i.updated DESC,p.item,p.position LIMIT {BATCH}"),vec![model.into(),workspace.clone().into()]).await?
            .into_iter()
            .map(|r| Ok((r.try_get("", "item")?, r.try_get("", "position")?, r.try_get("", "revision")?, r.try_get("", "text")?)))
            .collect::<Result<_>>()?;
        Ok(Some(Pending {
            workspace,
            passages,
        }))
    }

    /// Stores embeddings for passages that are still at the revision embedded.
    pub(crate) async fn library_store_embeddings(
        &self,
        workspace: &str,
        model: &str,
        embedded: &[((String, i32, i64), String)],
    ) -> Result<()> {
        let Some(schema) = vectors(&self.db).await? else {
            return Ok(());
        };
        let rows: Vec<Value> = embedded
            .iter()
            .map(|((item, position, revision), vector)| json!({"item":item,"position":position,"revision":revision,"vector":vector}))
            .collect();
        execute(&self.db,&format!("INSERT INTO relay_library_embeddings(workspace,item,position,revision,model,embedding) SELECT $1,p.item,p.position,p.revision,$2,(e.r->>'vector')::{schema}.vector FROM jsonb_array_elements($3::jsonb) AS e(r) JOIN relay_library_passages p ON p.workspace=$1 AND p.item=e.r->>'item' AND p.position=(e.r->>'position')::int AND p.revision=(e.r->>'revision')::bigint ON CONFLICT(workspace,item,position) DO UPDATE SET revision=EXCLUDED.revision,model=EXCLUDED.model,embedding=EXCLUDED.embedding"),vec![workspace.into(),model.into(),json!(rows).into()]).await?;
        Ok(())
    }

    /// Up to a few documents whose summary is missing or older than their revision.
    pub(crate) async fn library_pending_summaries(&self, limit: i64) -> Result<Vec<Value>> {
        json_rows(&self.db,&format!("SELECT jsonb_build_object('workspace',i.workspace,'item',i.id,'revision',i.revision,'title',i.title,'text',(SELECT left(string_agg(p.text,E'\\n' ORDER BY p.position),6000) FROM relay_library_passages p WHERE p.workspace=i.workspace AND p.item=i.id AND p.revision=i.revision)) AS data FROM relay_library_items i JOIN relay_library_indexed x ON x.workspace=i.workspace AND x.item=i.id AND x.revision=i.revision LEFT JOIN relay_library_summaries s ON s.workspace=i.workspace AND s.item=i.id AND s.revision=i.revision WHERE s.item IS NULL AND NOT i.trashed AND i.kind<>'folder' AND EXISTS(SELECT 1 FROM relay_library_passages p WHERE p.workspace=i.workspace AND p.item=i.id) ORDER BY i.updated DESC LIMIT {limit}"),vec![]).await
    }

    pub(crate) async fn library_store_summary(
        &self,
        workspace: &str,
        item: &str,
        revision: i64,
        model: &str,
        summary: &str,
    ) -> Result<()> {
        execute(&self.db,"INSERT INTO relay_library_summaries(workspace,item,revision,model,summary) SELECT $1,$2,$3,$4,$5 FROM relay_library_items WHERE workspace=$1 AND id=$2 AND revision=$3 ON CONFLICT(workspace,item) DO UPDATE SET revision=EXCLUDED.revision,model=EXCLUDED.model,summary=EXCLUDED.summary",vec![workspace.into(),item.into(),revision.into(),model.into(),summary.into()]).await?;
        Ok(())
    }
}

/// The passages nearest a question's meaning among documents this agent may use for
/// this reply, in the same shape as word matches.
pub(super) async fn nearest(
    tx: &DatabaseTransaction,
    job: &Job,
    vector: &str,
    folder: &str,
    model: &str,
) -> Result<Vec<Value>> {
    let Some(schema) = vectors(tx).await? else {
        return Ok(vec![]);
    };
    json_rows(tx,&format!("WITH hits AS MATERIALIZED (SELECT p.item,p.position,p.section,p.text,p.revision,i.title,i.access_revision,CASE WHEN i.folder IS NULL THEN NULL ELSE s.title END AS folder,e.embedding OPERATOR({schema}.<=>) $4::{schema}.vector AS distance
  FROM relay_library_embeddings e JOIN relay_library_passages p ON p.workspace=e.workspace AND p.item=e.item AND p.position=e.position AND p.revision=e.revision
  JOIN relay_library_items i ON i.workspace=p.workspace AND i.id=p.item AND NOT i.trashed AND i.revision=p.revision
  JOIN relay_library_items s ON s.workspace=i.workspace AND s.id=COALESCE(i.folder,i.id) AND NOT s.trashed
  JOIN relay_library_agents g ON g.workspace=s.workspace AND g.item=s.id AND g.agent=$2
  WHERE e.workspace=$1 AND e.model=$6 AND ($5='' OR s.id=$5) ORDER BY distance,p.item,p.position LIMIT 200),
 usable AS (SELECT item FROM (SELECT DISTINCT item FROM hits) d WHERE relay_library_agent_use($1,d.item,$2,$3) IS NOT NULL)
 SELECT jsonb_build_object('id',h.item,'title',h.title,'folder',h.folder,'section',h.section,'passage',h.position,'revision',h.revision,'access_revision',h.access_revision,
  'preview',left(h.text,300),'summary',(SELECT m.summary FROM relay_library_summaries m WHERE m.workspace=$1 AND m.item=h.item AND m.revision=h.revision)) AS data
 FROM hits h WHERE h.item IN (SELECT item FROM usable) AND h.distance<0.75 ORDER BY h.distance,h.item,h.position LIMIT 60"),vec![job.workspace.clone().into(),job.agent.clone().into(),job.reply.clone().into(),vector.into(),folder.into(),model.into()]).await
}

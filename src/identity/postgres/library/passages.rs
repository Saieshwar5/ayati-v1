//! Library passages (LB2a): every page and file is kept cut into passages for search
//! by section. Writes index the item in their own transaction; anything older or
//! missed is caught up, a bounded batch at a time, before agents search.
use super::*;
use crate::identity::library::passages::{page_text, split};
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};

/// Schema 70: passages, the revision each item was indexed at, and search receipts.
pub(crate) struct LibraryPassages;
impl MigrationName for LibraryPassages {
    fn name(&self) -> &str {
        "m20261105_000070_library_passages"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for LibraryPassages {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("passages.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Library passages require a forward migration".into(),
        ))
    }
}

const CATCH_UP: i64 = 100;

/// Before schema 70 there is nothing to index.
pub(super) async fn present(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regclass('relay_library_passages') IS NOT NULL AS present",
        vec![],
    )
    .await?
    .context("Library passages schema check missing")?
    .try_get("", "present")?)
}

/// Replaces an item's passages with ones cut from its current revision.
pub(in crate::identity::postgres) async fn index_item(
    db: &impl ConnectionTrait,
    w: &str,
    id: &str,
) -> Result<()> {
    if !present(db).await? {
        return Ok(());
    }
    let Some(row) = query(db,"SELECT kind,title,document,text,revision FROM relay_library_items WHERE workspace=$1 AND id=$2",vec![w.into(),id.into()]).await? else {
        return Ok(());
    };
    let kind: String = row.try_get("", "kind")?;
    let revision: i64 = row.try_get("", "revision")?;
    let text = match kind.as_str() {
        "page" => page_text(
            &row.try_get::<Option<Value>>("", "document")?
                .unwrap_or(Value::Null),
        ),
        "file" => row.try_get("", "text")?,
        _ => String::new(),
    };
    let passages: Vec<Value> = split(&text)
        .into_iter()
        .map(|p| json!({"section": p.section, "text": p.text}))
        .collect();
    execute(
        db,
        "DELETE FROM relay_library_passages WHERE workspace=$1 AND item=$2",
        vec![w.into(), id.into()],
    )
    .await?;
    execute(db,"INSERT INTO relay_library_passages(workspace,item,position,revision,title,section,text) SELECT $1,$2,(e.n-1)::int,$3,$4,e.p->>'section',e.p->>'text' FROM jsonb_array_elements($5::jsonb) WITH ORDINALITY AS e(p,n)",vec![w.into(),id.into(),revision.into(),row.try_get::<String>("","title")?.into(),json!(passages).into()]).await?;
    execute(db,"INSERT INTO relay_library_indexed(workspace,item,revision) VALUES($1,$2,$3) ON CONFLICT(workspace,item) DO UPDATE SET revision=EXCLUDED.revision",vec![w.into(),id.into(),revision.into()]).await?;
    Ok(())
}

/// Indexes up to a batch of items whose passages are older than their revision.
pub(in crate::identity::postgres) async fn catch_up(
    db: &impl ConnectionTrait,
    w: &str,
) -> Result<()> {
    if !present(db).await? {
        return Ok(());
    }
    let stale = rows(db,&format!("SELECT i.id FROM relay_library_items i LEFT JOIN relay_library_indexed x ON x.workspace=i.workspace AND x.item=i.id WHERE i.workspace=$1 AND i.kind<>'folder' AND NOT i.trashed AND x.revision IS DISTINCT FROM i.revision ORDER BY i.updated DESC LIMIT {CATCH_UP}"),vec![w.into()]).await?;
    for row in stale {
        index_item(db, w, &row.try_get::<String>("", "id")?).await?;
    }
    Ok(())
}

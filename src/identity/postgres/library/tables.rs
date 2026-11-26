//! Library tables (LB4): people save whole tables as versions; agents look a row up by
//! its key (cited) and, with Edit, change one row at a time as a new version.
use super::*;
use crate::identity::chat::model::Job;
use crate::identity::library::tables;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use serde::Deserialize;

/// Schema 76: the table kind.
pub(crate) struct LibraryTables;
impl MigrationName for LibraryTables {
    fn name(&self) -> &str {
        "m20261105_000076_library_tables"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for LibraryTables {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("tables.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Library tables require a forward migration".into(),
        ))
    }
}

#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SaveTable {
    pub id: String,
    pub request: String,
    /// 0 to create; otherwise the revision this change was made from.
    pub revision: i64,
    pub title: String,
    pub table: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Lookup {
    table: String,
    key: Value,
}

impl PgStore {
    pub(crate) async fn library_table_save(
        &self,
        user: &User,
        w: &str,
        input: &SaveTable,
    ) -> Result<Value> {
        uuid(&input.id)?;
        title(&input.title)?;
        let text = tables::validate(&input.table)?;
        let payload = json!({"table":input});
        let tx = self.workspace_tx(w).await?;
        role(&tx, w, &user.id).await?;
        if let Some(saved) = retry(&tx, user, w, &input.request, &payload).await? {
            tx.commit().await?;
            return Ok(saved);
        }
        if input.revision == 0 {
            let count: i64 = query(
                &tx,
                "SELECT count(*) AS n FROM relay_library_items WHERE workspace=$1",
                vec![w.into()],
            )
            .await?
            .context("Count")?
            .try_get("", "n")?;
            ensure!(
                count < MAX_ITEMS,
                "This workspace's Library limit is 10,000 items"
            );
            if let Some(folder) = &input.folder {
                super::folders::open_folder(&tx, user, w, folder).await?;
            }
            execute(&tx, "INSERT INTO relay_library_items(workspace,id,kind,title,created_by,steward,document,text,created,updated,folder) VALUES($1,$2,'table',$3,$4,$4,$5,$6,$7,$7,$8)", vec![w.into(), input.id.clone().into(), input.title.trim().into(), user.id.clone().into(), input.table.clone().into(), text.clone().into(), now().into(), input.folder.clone().into()]).await?;
        } else {
            let current = item(&tx, user, w, &input.id, false).await?;
            ensure!(current["kind"] == "table", "This item is not a table");
            ensure!(
                current["editable"] == true,
                "Only editors can change this table"
            );
            if current["revision"].as_i64() != Some(input.revision) {
                return Err(Conflict.into());
            }
            execute(&tx, "UPDATE relay_library_items SET title=$3,document=$4,text=$5,revision=revision+1,updated=$6 WHERE workspace=$1 AND id=$2", vec![w.into(), input.id.clone().into(), input.title.trim().into(), input.table.clone().into(), text.clone().into(), now().into()]).await?;
        }
        let revision = input.revision + 1;
        execute(&tx, "INSERT INTO relay_library_versions(workspace,item,revision,author,title,document,text,created) VALUES($1,$2,$3,$4,$5,$6,$7,$8)", vec![w.into(), input.id.clone().into(), revision.into(), user.id.clone().into(), input.title.trim().into(), input.table.clone().into(), text.into(), now().into()]).await?;
        super::passages::index_item(&tx, w, &input.id).await?;
        super::pages::prune_versions(&tx, w, &input.id).await?;
        let result = json!({"id":input.id,"revision":revision});
        remember(&tx, user, w, &input.request, payload, &result).await?;
        audit_with_summary(
            &tx,
            &user.id,
            Some(w),
            "library_table_saved",
            &input.id,
            &format!(
                "Saved the table \"{}\" (revision {revision})",
                input.title.trim()
            ),
        )
        .await?;
        tx.commit().await?;
        Ok(result)
    }

    /// An exact row by key, from a table this agent may use for this reply; cited.
    pub(crate) async fn library_lookup(&self, job: &Job, input: &Lookup) -> Result<Value> {
        let tx = self.workspace_tx(&job.workspace).await?;
        super::super::control_actions::authorize(&tx, job, true).await?;
        let found = json_rows(&tx, "SELECT jsonb_build_object('id',i.id,'title',i.title,'revision',i.revision,'access_revision',i.access_revision,'table',i.document) AS data FROM relay_library_items i WHERE i.workspace=$1 AND i.id=$2 AND i.kind='table' AND NOT i.trashed AND relay_library_agent_use(i.workspace,i.id,$3,$4) IS NOT NULL", vec![job.workspace.clone().into(), input.table.clone().into(), job.agent.clone().into(), job.reply.clone().into()]).await?.into_iter().next();
        let Some(found) = found else {
            tx.commit().await?;
            return Ok(
                json!({"state":"not_found","message":"No such table among the Library items you can use for the people in this conversation."}),
            );
        };
        let row = tables::row(&found["table"], &input.key);
        let first = found["table"]["columns"][0]["name"]
            .as_str()
            .unwrap_or("")
            .to_owned();
        // Cite the row by its own key, as the table writes it.
        let label: String = row
            .as_ref()
            .and_then(|r| r[&first].as_str())
            .unwrap_or("")
            .chars()
            .take(80)
            .collect();
        if row.is_some() {
            execute(&tx,"INSERT INTO relay_library_reads(workspace,reply,item,revision,access_revision,title,at,cited,section) VALUES($1,$2,$3,$4,$5,$6,$7,true,$8) ON CONFLICT(workspace,reply,item) DO UPDATE SET revision=EXCLUDED.revision,access_revision=EXCLUDED.access_revision,at=EXCLUDED.at,cited=true,section=EXCLUDED.section",vec![job.workspace.clone().into(),job.reply.clone().into(),input.table.clone().into(),found["revision"].as_i64().unwrap_or(1).into(),found["access_revision"].as_i64().unwrap_or(1).into(),found["title"].as_str().unwrap_or("").into(),now().into(),label.into()]).await?;
        } else {
            execute(&tx,"INSERT INTO relay_library_reads(workspace,reply,item,revision,access_revision,title,at,cited,section) VALUES($1,$2,$3,$4,$5,$6,$7,false,'') ON CONFLICT(workspace,reply,item) DO NOTHING",vec![job.workspace.clone().into(),job.reply.clone().into(),input.table.clone().into(),found["revision"].as_i64().unwrap_or(1).into(),found["access_revision"].as_i64().unwrap_or(1).into(),found["title"].as_str().unwrap_or("").into(),now().into()]).await?;
        }
        tx.commit().await?;
        let columns: Vec<Value> = found["table"]["columns"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        Ok(match row {
            Some(row) => {
                json!({"state":"found","table":found["title"],"revision":found["revision"],"row":row})
            }
            None => {
                json!({"state":"no_row","table":found["title"],"revision":found["revision"],"columns":columns,"message":"No row has this key. Keys are matched exactly, ignoring case; say so rather than guessing."})
            }
        })
    }
}

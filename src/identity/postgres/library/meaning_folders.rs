//! Meaning search per folder (schema 156). Vectors are not made by default: only what is
//! in a folder whose manager turned meaning search on is sent to the embedding and
//! summary models. Everything else is found by words, exact search or by opening it.
//! An agent that keeps missing in a folder may propose turning it on; a person decides,
//! because the folder's text then goes to the model provider.
use super::agent_tools::{USABLE, binds};
use super::*;
use crate::identity::chat::model::Job;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use serde::Deserialize;

pub(crate) struct LibraryMeaningFolders;
impl MigrationName for LibraryMeaningFolders {
    fn name(&self) -> &str {
        "m20261125_000156_library_meaning_folders"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for LibraryMeaningFolders {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("meaning_folders.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Meaning search per folder requires a forward migration".into(),
        ))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MeaningSwitch {
    pub on: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProposeMeaning {
    folder: String,
    reason: String,
}

/// Whether any folder in the workspace has meaning search on: without one, a question's
/// meaning is never asked of the model.
pub(super) async fn any_on(db: &impl ConnectionTrait, w: &str) -> Result<bool> {
    Ok(query(db, "SELECT 1 AS found FROM relay_library_items WHERE workspace=$1 AND kind='folder' AND meaning AND NOT trashed LIMIT 1", vec![w.into()]).await?.is_some())
}

impl PgStore {
    /// A folder's meaning search: whether it is on, whether a model is set up, and what
    /// agents proposed.
    pub(crate) async fn library_meaning(&self, user: &User, w: &str, id: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let folder = item(&tx, user, w, id, false).await?;
        ensure!(
            folder["kind"] == "folder",
            "Meaning search is set on folders"
        );
        let manages = folder["manageable"] == true;
        let proposals = if manages {
            json_rows(&tx, "SELECT jsonb_build_object('agent',a.name,'reason',p.reason,'at',p.at) AS data FROM relay_library_meaning_proposals p JOIN relay_shared_agents a ON a.workspace=p.workspace AND a.id=p.agent WHERE p.workspace=$1 AND p.folder=$2 ORDER BY p.at", vec![w.into(), id.into()]).await?
        } else {
            vec![]
        };
        let inherited: bool = query(
            &tx,
            "SELECT relay_library_meaning($1,$2) AS on",
            vec![w.into(), id.into()],
        )
        .await?
        .context("Folder unavailable")?
        .try_get("", "on")?;
        tx.commit().await?;
        Ok(json!({
            "on": folder["meaning"] == true,
            "from_folder_above": inherited && folder["meaning"] != true,
            "available": self.library_models.as_ref().is_some_and(|m| m.embedding().is_some()),
            "can_change": manages,
            "proposals": proposals,
        }))
    }

    /// The folder's manager turns meaning search on or off. Off removes what was made.
    pub(crate) async fn library_meaning_set(
        &self,
        user: &User,
        w: &str,
        id: &str,
        input: &MeaningSwitch,
    ) -> Result<Value> {
        let tx = self.workspace_tx(w).await?;
        let folder = item(&tx, user, w, id, false).await?;
        ensure!(
            folder["kind"] == "folder",
            "Meaning search is set on folders"
        );
        ensure!(
            folder["manageable"] == true,
            "Only the folder's manager turns meaning search on or off"
        );
        agent_folders::not_agents(&tx, w, id).await?;
        execute(
            &tx,
            "UPDATE relay_library_items SET meaning=$3 WHERE workspace=$1 AND id=$2",
            vec![w.into(), id.into(), input.on.into()],
        )
        .await?;
        execute(
            &tx,
            "DELETE FROM relay_library_meaning_proposals WHERE workspace=$1 AND folder=$2",
            vec![w.into(), id.into()],
        )
        .await?;
        if !input.on && meaning::vectors(&tx).await?.is_some() {
            execute(&tx, "DELETE FROM relay_library_embeddings e WHERE e.workspace=$1 AND NOT relay_library_meaning(e.workspace,e.item)", vec![w.into()]).await?;
        }
        let title = folder["title"].as_str().unwrap_or("");
        let said = if input.on {
            format!(
                "Turned on meaning search for \"{title}\": its text is sent to the model provider"
            )
        } else {
            format!("Turned off meaning search for \"{title}\"")
        };
        audit_with_summary(&tx, &user.id, Some(w), "library_meaning", id, &said).await?;
        tx.commit().await?;
        Ok(json!({"ok":true}))
    }

    /// An agent proposes meaning search for a folder it was given; the manager decides.
    pub(crate) async fn library_propose_meaning(
        &self,
        job: &Job,
        input: &ProposeMeaning,
    ) -> Result<Value> {
        let reason = input.reason.trim();
        if reason.is_empty() || reason.chars().count() > 300 {
            return Ok(json!({"state":"invalid","message":"Say why in up to 300 characters."}));
        }
        let tx = self.workspace_tx(&job.workspace).await?;
        super::super::control_actions::authorize(&tx, job, true).await?;
        let mut values = binds(job);
        values.push(input.folder.clone().into());
        let found = query(&tx, &format!("SELECT relay_library_meaning(i.workspace,i.id) AS on FROM {USABLE} AND i.id=$4 AND i.kind='folder'"), values).await?;
        let Some(found) = found else {
            tx.commit().await?;
            return Ok(
                json!({"state":"not_found","message":"No such folder among the Library items you can use here."}),
            );
        };
        if found.try_get::<bool>("", "on")? {
            tx.commit().await?;
            return Ok(
                json!({"state":"already_on","message":"Meaning search is already on for this folder."}),
            );
        }
        execute(&tx, "INSERT INTO relay_library_meaning_proposals(workspace,folder,agent,reason,at) VALUES($1,$2,$3,$4,$5) ON CONFLICT(workspace,folder,agent) DO UPDATE SET reason=EXCLUDED.reason,at=EXCLUDED.at", vec![job.workspace.clone().into(), input.folder.clone().into(), job.agent.clone().into(), reason.into(), now().into()]).await?;
        tx.commit().await?;
        Ok(
            json!({"state":"proposed","next":"The folder's manager sees your proposal on the folder and decides. Until then, search with other words or open the document."}),
        )
    }
}

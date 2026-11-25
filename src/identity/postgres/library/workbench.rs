//! The agent's workbench (schema 154): the Library keeps, the agent's computer works.
//! `open_document` puts a Library item on the agent's computer as a file (a file as it
//! is, a page as Markdown, a table as CSV) and remembers the revision it came from;
//! `save_file` saves a file from the computer back as that item's next version, or as a
//! new file in a folder. Opening follows the agent's grant and audience and leaves a
//! read receipt; saving needs the agent's level and the person it works for to be
//! allowed the change, never overwrites a newer revision, and is a version marked with
//! the agent that History can restore.
use super::super::computer_tools::{resolve, shown};
use super::agent_tools::{USABLE, binds, receipt};
use super::*;
use crate::identity::chat::model::Job;
use crate::identity::library::{page_markdown, tables_csv};
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use serde::Deserialize;

pub(crate) struct LibraryWorkbench;
impl MigrationName for LibraryWorkbench {
    fn name(&self) -> &str {
        "m20261125_000154_library_workbench"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for LibraryWorkbench {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("workbench.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "The Library workbench requires a forward migration".into(),
        ))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OpenDocument {
    id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SaveFile {
    pub(super) path: String,
    /// For a new file: the folder to save it in.
    #[serde(default)]
    pub(super) folder: Option<String>,
    /// For a new file: its name in the Library; the file's own name otherwise.
    #[serde(default)]
    pub(super) title: Option<String>,
    /// Why, when the change is a suggestion.
    #[serde(default)]
    pub(super) note: String,
}

pub(super) fn invalid(message: &str) -> Value {
    json!({"state":"invalid","message":message})
}
pub(super) fn changed(revision: i64) -> Value {
    json!({"state":"changed","revision":revision,"message":"It changed in the Library since you opened it. Open it again and make your change on the new revision."})
}
pub(super) fn not_allowed(level: u8) -> Value {
    json!({"state":"not_allowed","message":if level >= 1 { "You may suggest changes here but not save them. Tell the person what you would change." } else { "You may only read this. Tell the person what you would change." }})
}

/// The file's name on the computer: the title, safe for a path, with the ending its
/// kind is worked on as.
fn file_name(title: &str, kind: &str) -> String {
    let mut name: String = title
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || " .-_()".contains(c) {
                c
            } else {
                '_'
            }
        })
        .take(120)
        .collect();
    if name.trim_matches(['.', ' ', '_']).is_empty() {
        name = "document".into();
    }
    let ending = match kind {
        "page" => ".md",
        "table" => ".csv",
        _ => "",
    };
    if !ending.is_empty() && !name.to_lowercase().ends_with(ending) {
        name.push_str(ending);
    }
    name
}

impl PgStore {
    pub(crate) async fn library_open_document(
        &self,
        job: &Job,
        input: &OpenDocument,
    ) -> Result<Value> {
        let w = &job.workspace;
        let tx = self.workspace_tx(w).await?;
        super::super::control_actions::authorize(&tx, job, true).await?;
        let mut values = binds(job);
        values.push(input.id.clone().into());
        let found = json_rows(&tx, &format!("SELECT jsonb_build_object('id',i.id,'title',i.title,'kind',i.kind,'revision',i.revision,'access_revision',i.access_revision,'object_key',i.object_key,'size',i.size,'checksum',i.checksum,'document',i.document) AS data FROM {USABLE} AND i.id=$4 AND i.kind<>'folder'"), values).await?.into_iter().next();
        let Some(found) = found else {
            tx.commit().await?;
            return Ok(
                json!({"state":"not_found","message":"No such document among the Library items you can use here."}),
            );
        };
        // Having the whole item is reading it: it fences and is cited like a read.
        receipt(&tx, job, &found, true, "").await?;
        tx.commit().await?;
        let kind = found["kind"].as_str().unwrap_or("file");
        let bytes = match kind {
            "page" => page_markdown::to_markdown(&found["document"]).into_bytes(),
            "table" => tables_csv::to_csv(&found["document"]).into_bytes(),
            _ => {
                let key = found["object_key"]
                    .as_str()
                    .context("Stored file missing")?;
                let bytes = self.library_files.get(key).await?;
                ensure!(
                    bytes.len() as i64 == found["size"].as_i64().unwrap_or(-1)
                        && checksum(&bytes) == found["checksum"],
                    "Stored file failed its integrity check"
                );
                bytes.to_vec()
            }
        };
        let ready = match self.computer_ready(job).await? {
            Ok(ready) => ready,
            Err(refusal) => return Ok(refusal),
        };
        // Two items with one name, opened in one request, get files of their own.
        let name = file_name(found["title"].as_str().unwrap_or(""), kind);
        let mut relative = format!("library/{name}");
        let taken = query(
            &self.db,
            "SELECT item FROM relay_library_opened WHERE workspace=$1 AND reply=$2 AND path=$3",
            vec![
                w.clone().into(),
                job.reply.clone().into(),
                relative.clone().into(),
            ],
        )
        .await?;
        if taken
            .is_some_and(|row| row.try_get::<String>("", "item").ok().as_deref() != Some(&input.id))
        {
            relative = format!(
                "library/{}-{name}",
                input.id.chars().take(8).collect::<String>()
            );
        }
        let path = match resolve(job, &relative) {
            Ok(path) => path,
            Err(refusal) => return Ok(refusal),
        };
        let size = bytes.len();
        self.computer_put(&ready, &path, bytes).await?;
        self.computer_used(job).await?;
        self.computer_step(
            job,
            "write",
            shown(&path),
            "completed",
            None,
            "",
            size as i64,
            0,
        )
        .await?;
        let revision = found["revision"].as_i64().unwrap_or(1);
        self.opened(job, &relative, &input.id, revision).await?;
        Ok(
            json!({"state":"opened","path":relative,"title":found["title"],"kind":kind,"revision":revision,"bytes":size,
            "next":"Work on this file with computer_run. To keep a change, call save_file with this path: it becomes the next version in the Library."}),
        )
    }

    /// Remembers which item and revision a file on the computer is.
    pub(super) async fn opened(
        &self,
        job: &Job,
        path: &str,
        item: &str,
        revision: i64,
    ) -> Result<()> {
        execute(&self.db, "INSERT INTO relay_library_opened(workspace,reply,path,item,revision,at,agent) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(workspace,reply,path) DO UPDATE SET item=EXCLUDED.item,revision=EXCLUDED.revision,at=EXCLUDED.at", vec![job.workspace.clone().into(), job.reply.clone().into(), path.into(), item.into(), revision.into(), now().into(), job.agent.clone().into()]).await?;
        Ok(())
    }

    /// Removes what earlier requests opened from the Library before this request uses
    /// the computer. A copy is for the people of the request that opened it; the next
    /// request may be for someone who can't see that document.
    pub(in crate::identity::postgres) async fn workbench_tidy(
        &self,
        job: &Job,
        ready: &super::super::computers::Ready,
    ) -> Result<()> {
        // Upgrade tests use computers at schemas before the workbench.
        let present: bool = query(
            &self.db,
            "SELECT to_regclass('relay_library_opened') IS NOT NULL AS present",
            vec![],
        )
        .await?
        .context("Schema check")?
        .try_get("", "present")?;
        if !present {
            return Ok(());
        }
        let earlier: Vec<String> = rows(&self.db, "SELECT DISTINCT reply FROM relay_library_opened WHERE workspace=$1 AND agent=$2 AND reply<>$3 LIMIT 50", vec![job.workspace.clone().into(), job.agent.clone().into(), job.reply.clone().into()]).await?.into_iter().map(|r| r.try_get("", "reply")).collect::<std::result::Result<_, _>>()?;
        for reply in earlier {
            if reply.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                self.computer_exec(
                    ready,
                    &format!("rm -rf 'requests/{reply}/library'"),
                    crate::identity::computers::HOME,
                    std::time::Duration::from_secs(30),
                )
                .await?;
            }
            execute(
                &self.db,
                "DELETE FROM relay_library_opened WHERE workspace=$1 AND agent=$2 AND reply=$3",
                vec![
                    job.workspace.clone().into(),
                    job.agent.clone().into(),
                    reply.into(),
                ],
            )
            .await?;
        }
        Ok(())
    }
}

//! Saving from the agent's computer to the Library (schema 154): a file that was opened
//! becomes its item's next version, any other file a new file in a folder. See
//! `workbench` for opening and the rules.
use super::super::computer_tools::resolve;
use super::agent_tools::link;
use super::edits::{name_of, standing, unavailable};
use super::workbench::{SaveFile, changed, invalid, not_allowed};
use super::*;
use crate::identity::chat::model::Job;
use crate::identity::library::{file_mime, file_text, page_markdown, tables, tables_csv};

impl PgStore {
    pub(crate) async fn library_save_file(&self, job: &Job, input: &SaveFile) -> Result<Value> {
        let relative = input.path.trim().trim_start_matches("./").to_owned();
        let path = match resolve(job, &relative) {
            Ok(path) => path,
            Err(refusal) => return Ok(refusal),
        };
        if input.note.chars().count() > 500 {
            return Ok(invalid("Keep the note under 500 characters."));
        }
        let ready = match self.computer_ready(job).await? {
            Ok(ready) => ready,
            Err(refusal) => return Ok(refusal),
        };
        let bytes = self.computer_get(&ready, &path).await;
        self.computer_used(job).await?;
        let Ok(bytes) = bytes else {
            return Ok(invalid(&format!(
                "There is no file {relative} on your computer."
            )));
        };
        if bytes.is_empty() || bytes.len() > MAX_FILE {
            return Ok(invalid("Save a file between 1 byte and 25 MiB."));
        }
        let from = query(&self.db, "SELECT item,revision FROM relay_library_opened WHERE workspace=$1 AND reply=$2 AND path=$3", vec![job.workspace.clone().into(), job.reply.clone().into(), relative.clone().into()]).await?;
        let saved = match from {
            Some(row) => {
                let item: String = row.try_get("", "item")?;
                self.save_version(job, &item, row.try_get("", "revision")?, bytes, &input.note)
                    .await?
            }
            None => self.save_new(job, input, &relative, bytes).await?,
        };
        // Saving the same file again continues from the revision just written.
        if let (Some(id), Some(revision), false) = (
            saved["id"].as_str(),
            saved["revision"].as_i64(),
            saved["state"] == "changed",
        ) {
            self.opened(job, &relative, id, revision).await?;
        }
        Ok(saved)
    }

    async fn save_version(
        &self,
        job: &Job,
        id: &str,
        base: i64,
        bytes: Vec<u8>,
        note: &str,
    ) -> Result<Value> {
        let w = &job.workspace;
        let tx = self.workspace_tx(w).await?;
        super::super::control_actions::authorize(&tx, job, true).await?;
        let Some((level, access, item)) = standing(&tx, job, id).await? else {
            tx.commit().await?;
            return Ok(unavailable());
        };
        tx.commit().await?;
        let may_edit = level >= 2 && access.as_deref() == Some("edit");
        let as_text = || String::from_utf8(bytes.clone());
        match item["kind"].as_str().unwrap_or("") {
            "page" => {
                let Ok(text) = as_text() else {
                    return Ok(invalid("A page is saved from a text file with Markdown."));
                };
                let document = page_markdown::from_markdown(&text);
                let plain = match document_text(&document) {
                    Ok(plain) => plain,
                    Err(error) => return Ok(invalid(&error.to_string())),
                };
                let mut saved = if may_edit {
                    self.library_edit_page(job, id, Some(base), document, plain)
                        .await?
                } else {
                    self.library_suggest_page(job, id, note, document, plain)
                        .await?
                };
                if saved["revision"].is_i64() {
                    saved["id"] = json!(id);
                }
                Ok(saved)
            }
            "table" => {
                if !may_edit {
                    return Ok(not_allowed(level));
                }
                let Ok(text) = as_text() else {
                    return Ok(invalid("A table is saved from a CSV text file."));
                };
                let table = match tables_csv::from_csv(&text, Some(&item["document"])) {
                    Ok(table) => table,
                    Err(error) => return Ok(invalid(&error.to_string())),
                };
                let plain = match tables::validate(&table) {
                    Ok(plain) => plain,
                    Err(error) => return Ok(invalid(&error.to_string())),
                };
                let tx = self.workspace_tx(w).await?;
                let Some(current) = self.still_editable(&tx, job, id).await? else {
                    tx.commit().await?;
                    return Ok(not_allowed(level));
                };
                if current.0 != base {
                    tx.commit().await?;
                    return Ok(changed(current.0));
                }
                let revision = super::edits::write_revision(
                    &tx,
                    w,
                    id,
                    &job.requester,
                    &current.1,
                    &table,
                    &plain,
                    &job.agent,
                    &job.reply,
                )
                .await?;
                self.saved_by_agent(
                    &tx,
                    job,
                    id,
                    &format!(
                        "saved a new version of \"{}\" (revision {revision})",
                        current.1
                    ),
                )
                .await?;
                tx.commit().await?;
                Ok(json!({"state":"saved","id":id,"revision":revision,"link":link(w, id)}))
            }
            _ => {
                if !may_edit {
                    return Ok(not_allowed(level));
                }
                if item["revision"].as_i64() != Some(base) {
                    return Ok(changed(item["revision"].as_i64().unwrap_or(base)));
                }
                let Some(stored) = self.store_object(w, bytes).await? else {
                    return Ok(invalid(
                        "Library storage is full or unavailable. Tell the person.",
                    ));
                };
                let tx = self.workspace_tx(w).await?;
                let current = self.still_editable(&tx, job, id).await?;
                let Some((revision, title)) = current.filter(|c| c.0 == base) else {
                    // The object was stored for nothing: hand it back.
                    execute(&tx, "INSERT INTO relay_library_discarded(workspace,object_key,at) VALUES($1,$2,$3) ON CONFLICT DO NOTHING", vec![w.clone().into(), stored.key.clone().into(), now().into()]).await?;
                    tx.commit().await?;
                    return Ok(changed(base));
                };
                execute(&tx, "UPDATE relay_library_items SET text=$3,object_key=$4,mime=$5,size=$6,checksum=$7,readiness=$8,revision=revision+1,updated=$9 WHERE workspace=$1 AND id=$2", vec![w.clone().into(), id.into(), stored.text.into(), stored.key.into(), stored.mime.into(), stored.size.into(), stored.checksum.into(), stored.readiness.into(), now().into()]).await?;
                super::file_versions::snapshot(
                    &tx,
                    w,
                    id,
                    &job.requester,
                    Some((&job.agent, &job.reply)),
                )
                .await?;
                super::passages::index_item(&tx, w, id).await?;
                super::pages::prune_versions(&tx, w, id).await?;
                self.saved_by_agent(
                    &tx,
                    job,
                    id,
                    &format!(
                        "saved a new version of \"{title}\" (revision {})",
                        revision + 1
                    ),
                )
                .await?;
                tx.commit().await?;
                Ok(json!({"state":"saved","id":id,"revision":revision + 1,"link":link(w, id)}))
            }
        }
    }

    async fn save_new(
        &self,
        job: &Job,
        input: &SaveFile,
        relative: &str,
        bytes: Vec<u8>,
    ) -> Result<Value> {
        let w = &job.workspace;
        let Some(folder) = &input.folder else {
            return Ok(invalid(
                "This file didn't come from the Library. Name the folder to save it in (an id from list_folder).",
            ));
        };
        let name = input
            .title
            .as_deref()
            .unwrap_or_else(|| relative.rsplit('/').next().unwrap_or(relative))
            .trim()
            .to_owned();
        if title(&name).is_err() {
            return Ok(invalid("Give the file a name of 1 to 180 characters."));
        }
        let allowed = |standing: &Option<(u8, Option<String>, Value)>| {
            standing.as_ref().is_some_and(|(level, access, folder)| {
                folder["kind"] == "folder" && *level >= 3 && access.as_deref() == Some("edit")
            })
        };
        let refused = json!({"state":"not_allowed","message":"You can't save new files in this folder. Tell the person where the file is on your computer instead."});
        let tx = self.workspace_tx(w).await?;
        super::super::control_actions::authorize(&tx, job, true).await?;
        let found = standing(&tx, job, folder).await?;
        let count: i64 = query(
            &tx,
            "SELECT count(*) AS n FROM relay_library_items WHERE workspace=$1",
            vec![w.clone().into()],
        )
        .await?
        .context("Count")?
        .try_get("", "n")?;
        tx.commit().await?;
        if found.is_none() {
            return Ok(
                json!({"state":"not_found","message":"No such folder among the Library items you can use here."}),
            );
        }
        if !allowed(&found) {
            return Ok(refused);
        }
        ensure!(
            count < 1000,
            "This workspace's initial Library limit is 1,000 items"
        );
        let Some(stored) = self.store_object(w, bytes).await? else {
            return Ok(invalid(
                "Library storage is full or unavailable. Tell the person.",
            ));
        };
        let id = uuid::Uuid::new_v4().to_string();
        let tx = self.workspace_tx(w).await?;
        super::super::control_actions::authorize(&tx, job, true).await?;
        let found = standing(&tx, job, folder).await?;
        if !allowed(&found) {
            execute(&tx, "INSERT INTO relay_library_discarded(workspace,object_key,at) VALUES($1,$2,$3) ON CONFLICT DO NOTHING", vec![w.clone().into(), stored.key.clone().into(), now().into()]).await?;
            tx.commit().await?;
            return Ok(refused);
        }
        execute(&tx, "INSERT INTO relay_library_items(workspace,id,kind,title,created_by,steward,text,object_key,mime,size,checksum,readiness,created,updated,folder) VALUES($1,$2,'file',$3,$4,$4,$5,$6,$7,$8,$9,$10,$11,$11,$12)", vec![w.clone().into(), id.clone().into(), name.clone().into(), job.requester.clone().into(), stored.text.into(), stored.key.into(), stored.mime.into(), stored.size.into(), stored.checksum.into(), stored.readiness.into(), now().into(), folder.clone().into()]).await?;
        super::file_versions::snapshot(&tx, w, &id, &job.requester, Some((&job.agent, &job.reply)))
            .await?;
        super::passages::index_item(&tx, w, &id).await?;
        let place = found
            .map(|f| f.2["title"].as_str().unwrap_or("").to_owned())
            .unwrap_or_default();
        self.saved_by_agent(&tx, job, &id, &format!("saved \"{name}\" in \"{place}\""))
            .await?;
        tx.commit().await?;
        Ok(json!({"state":"saved","id":id,"revision":1,"link":link(w, &id)}))
    }

    /// The item's revision and title when the agent may still change it for this person.
    async fn still_editable(
        &self,
        tx: &DatabaseTransaction,
        job: &Job,
        id: &str,
    ) -> Result<Option<(i64, String)>> {
        super::super::control_actions::authorize(tx, job, true).await?;
        Ok(standing(tx, job, id)
            .await?
            .filter(|(level, access, _)| *level >= 2 && access.as_deref() == Some("edit"))
            .map(|(_, _, item)| {
                (
                    item["revision"].as_i64().unwrap_or(1),
                    item["title"].as_str().unwrap_or("").to_owned(),
                )
            }))
    }

    async fn saved_by_agent(
        &self,
        tx: &DatabaseTransaction,
        job: &Job,
        id: &str,
        what: &str,
    ) -> Result<()> {
        let name = name_of(tx, &job.workspace, &job.agent).await?;
        audit_with_summary(
            tx,
            &job.requester,
            Some(&job.workspace),
            "library_file_saved_by_agent",
            id,
            &format!("{name} {what}"),
        )
        .await
    }

    /// Stores the bytes as a new object and reads what they are; none when there is no
    /// room or no file storage.
    async fn store_object(&self, w: &str, bytes: Vec<u8>) -> Result<Option<Stored>> {
        if !self.library_files.enabled()
            || super::file_versions::storage_used(&self.db, w).await? + bytes.len() as i64
                > WORKSPACE_QUOTA
        {
            return Ok(None);
        }
        let bytes = axum::body::Bytes::from(bytes);
        let key = format!("library/{w}/{}", uuid::Uuid::new_v4());
        self.library_files.put(&key, bytes.clone()).await?;
        let mime = file_mime(&bytes);
        let (text, readiness) = file_text(&bytes, mime).await;
        Ok(Some(Stored {
            key,
            mime,
            text,
            readiness,
            size: bytes.len() as i64,
            checksum: checksum(&bytes),
        }))
    }
}

struct Stored {
    key: String,
    mime: &'static str,
    text: String,
    readiness: &'static str,
    size: i64,
    checksum: String,
}

fn document_text(document: &Value) -> Result<String> {
    crate::identity::library::model::document(document)
}

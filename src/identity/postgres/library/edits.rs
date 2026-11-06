//! Agents changing the Library (LB3). An agent's level on a folder or item, each
//! including the ones before: read, suggest (a tracked suggestion a person accepts),
//! edit (a new version, marked with the agent and task), save (new pages in a folder).
//! The person the agent works for must be allowed the same change, "use for" must fit
//! the conversation, and every change is a version that History can restore.
use super::*;
use crate::identity::chat::model::Job;
use crate::identity::library::page_text;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use serde::Deserialize;

/// Schema 74: suggestions, and the agent and task behind a version.
pub(crate) struct LibraryEdits;
impl MigrationName for LibraryEdits {
    fn name(&self) -> &str {
        "m20261105_000074_library_edits"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for LibraryEdits {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("edits.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Agent editing requires a forward migration".into(),
        ))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Change {
    id: String,
    /// The revision the agent read; a newer page is not overwritten.
    #[serde(default)]
    revision: Option<i64>,
    text: String,
    #[serde(default)]
    note: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NewPage {
    folder: String,
    title: String,
    text: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Decision {
    pub request: String,
    pub action: String,
}

pub(super) fn rank(level: &str) -> u8 {
    match level {
        "suggest" => 1,
        "edit" => 2,
        "save" => 3,
        _ => 0,
    }
}

/// The agent's level on an item for this reply (none when "use for" doesn't fit), and
/// what the person it works for may do with it.
async fn standing(
    tx: &DatabaseTransaction,
    job: &Job,
    item: &str,
) -> Result<Option<(u8, Option<String>, Value)>> {
    let row = json_rows(tx, "SELECT jsonb_build_object('level',g.level,'access',relay_library_access(i.workspace,i.id,$3),'kind',i.kind,'title',i.title,'revision',i.revision,'folder',i.folder,'document',i.document) AS data FROM relay_library_items i JOIN relay_library_items s ON s.workspace=i.workspace AND s.id=COALESCE(i.folder,i.id) JOIN relay_library_agents g ON g.workspace=s.workspace AND g.item=s.id AND g.agent=$4 WHERE i.workspace=$1 AND i.id=$2 AND NOT i.trashed AND relay_library_agent_use(i.workspace,i.id,$4,$5) IS NOT NULL", vec![job.workspace.clone().into(), item.into(), job.requester.clone().into(), job.agent.clone().into(), job.reply.clone().into()]).await?.into_iter().next();
    Ok(row.map(|r| {
        (
            rank(r["level"].as_str().unwrap_or("")),
            r["access"].as_str().map(str::to_owned),
            r,
        )
    }))
}
fn unavailable() -> Value {
    json!({"state":"not_found","message":"No such page among the Library items you can change for the people in this conversation."})
}
fn checked(text: &str) -> Result<(Value, String)> {
    ensure!(
        text.chars().count() <= 60000,
        "Write at most 60,000 characters"
    );
    let document = page_text::document(text);
    let plain = crate::identity::library::model::document(&document)?;
    Ok((document, plain))
}
async fn name_of(tx: &DatabaseTransaction, w: &str, agent: &str) -> Result<String> {
    Ok(query(
        tx,
        "SELECT name FROM relay_shared_agents WHERE workspace=$1 AND id=$2",
        vec![w.into(), agent.into()],
    )
    .await?
    .map(|r| r.try_get("", "name"))
    .transpose()?
    .unwrap_or_else(|| "An agent".into()))
}
/// Writes a new revision of a page and its version, marked with the agent and task.
#[allow(clippy::too_many_arguments)]
async fn write_revision(
    tx: &DatabaseTransaction,
    w: &str,
    id: &str,
    author: &str,
    title: &str,
    document: &Value,
    text: &str,
    agent: &str,
    job: &str,
) -> Result<i64> {
    let revision: i64 = query(tx, "UPDATE relay_library_items SET title=$3,document=$4,text=$5,revision=revision+1,updated=$6 WHERE workspace=$1 AND id=$2 RETURNING revision", vec![w.into(), id.into(), title.into(), document.clone().into(), text.into(), now().into()]).await?.context("Page unavailable")?.try_get("", "revision")?;
    execute(tx, "INSERT INTO relay_library_versions(workspace,item,revision,author,title,document,text,created,agent,job) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)", vec![w.into(), id.into(), revision.into(), author.into(), title.into(), document.clone().into(), text.into(), now().into(), agent.into(), job.into()]).await?;
    super::passages::index_item(tx, w, id).await?;
    super::pages::prune_versions(tx, w, id).await?;
    Ok(revision)
}

impl PgStore {
    pub(crate) async fn library_suggest_edit(&self, job: &Job, input: &Change) -> Result<Value> {
        let (document, text) = checked(&input.text)?;
        ensure!(
            input.note.chars().count() <= 500,
            "Keep the note under 500 characters"
        );
        let tx = self.workspace_tx(&job.workspace).await?;
        super::super::control_actions::authorize(&tx, job, true).await?;
        let Some((level, access, page)) = standing(&tx, job, &input.id).await? else {
            tx.commit().await?;
            return Ok(unavailable());
        };
        if page["kind"] != "page" || level < 1 || access.is_none() {
            tx.commit().await?;
            return Ok(
                json!({"state":"not_allowed","message":"You may only read this document. Tell the person what you would change instead."}),
            );
        }
        let pending: i64 = query(&tx, "SELECT count(*) AS n FROM relay_library_suggestions WHERE workspace=$1 AND item=$2 AND state='pending'", vec![job.workspace.clone().into(), input.id.clone().into()]).await?.context("Count")?.try_get("", "n")?;
        ensure!(pending < 20, "This page already has 20 suggestions waiting");
        let id = uuid::Uuid::new_v4().to_string();
        execute(&tx, "INSERT INTO relay_library_suggestions(workspace,id,item,base_revision,title,document,text,note,agent,job,requester,created) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)", vec![job.workspace.clone().into(), id.clone().into(), input.id.clone().into(), page["revision"].as_i64().unwrap_or(1).into(), page["title"].as_str().unwrap_or("").into(), document.into(), text.into(), input.note.trim().into(), job.agent.clone().into(), job.reply.clone().into(), job.requester.clone().into(), now().into()]).await?;
        let name = name_of(&tx, &job.workspace, &job.agent).await?;
        audit_with_summary(
            &tx,
            &job.requester,
            Some(&job.workspace),
            "library_change_suggested",
            &input.id,
            &format!(
                "{name} suggested a change to \"{}\"",
                page["title"].as_str().unwrap_or("")
            ),
        )
        .await?;
        tx.commit().await?;
        Ok(
            json!({"state":"suggested","suggestion":id,"next":"Someone who can edit the page accepts or rejects it in the Library. Tell the person in one line."}),
        )
    }

    pub(crate) async fn library_edit_document(&self, job: &Job, input: &Change) -> Result<Value> {
        let (document, text) = checked(&input.text)?;
        let tx = self.workspace_tx(&job.workspace).await?;
        super::super::control_actions::authorize(&tx, job, true).await?;
        let Some((level, access, page)) = standing(&tx, job, &input.id).await? else {
            tx.commit().await?;
            return Ok(unavailable());
        };
        if page["kind"] != "page" || level < 2 || access.as_deref() != Some("edit") {
            tx.commit().await?;
            return Ok(
                json!({"state":"not_allowed","message":if level >= 1 && access.is_some() { "You can't change this page directly here; use suggest_edit." } else { "You may only read this document." }}),
            );
        }
        let current = page["revision"].as_i64().unwrap_or(1);
        if input.revision.is_some_and(|r| r != current) {
            tx.commit().await?;
            return Ok(
                json!({"state":"changed","revision":current,"message":"The page changed since you read it. Read it again and make your change on the new revision."}),
            );
        }
        let title = page["title"].as_str().unwrap_or("").to_owned();
        let revision = write_revision(
            &tx,
            &job.workspace,
            &input.id,
            &job.requester,
            &title,
            &document,
            &text,
            &job.agent,
            &job.reply,
        )
        .await?;
        let name = name_of(&tx, &job.workspace, &job.agent).await?;
        audit_with_summary(
            &tx,
            &job.requester,
            Some(&job.workspace),
            "library_page_edited_by_agent",
            &input.id,
            &format!("{name} edited \"{title}\" (revision {revision})"),
        )
        .await?;
        tx.commit().await?;
        Ok(
            json!({"state":"edited","revision":revision,"link":format!("/w/{}/?view=library&item={}",job.workspace,input.id),"next":"The change is saved as a new version; anyone who can edit the page can restore the earlier one from History."}),
        )
    }

    pub(crate) async fn library_save_document(&self, job: &Job, input: &NewPage) -> Result<Value> {
        title(&input.title)?;
        let (document, text) = checked(&input.text)?;
        let tx = self.workspace_tx(&job.workspace).await?;
        super::super::control_actions::authorize(&tx, job, true).await?;
        let Some((level, access, folder)) = standing(&tx, job, &input.folder).await? else {
            tx.commit().await?;
            return Ok(
                json!({"state":"not_found","message":"No such folder among the Library items you can use here."}),
            );
        };
        if folder["kind"] != "folder" || level < 3 || access.as_deref() != Some("edit") {
            tx.commit().await?;
            return Ok(
                json!({"state":"not_allowed","message":"You can't save new documents in this folder. Give the text to the person instead."}),
            );
        }
        let count: i64 = query(
            &tx,
            "SELECT count(*) AS n FROM relay_library_items WHERE workspace=$1",
            vec![job.workspace.clone().into()],
        )
        .await?
        .context("Count")?
        .try_get("", "n")?;
        ensure!(
            count < 1000,
            "This workspace's initial Library limit is 1,000 items"
        );
        let id = uuid::Uuid::new_v4().to_string();
        execute(&tx, "INSERT INTO relay_library_items(workspace,id,kind,title,created_by,steward,document,text,created,updated,folder) VALUES($1,$2,'page',$3,$4,$4,$5,$6,$7,$7,$8)", vec![job.workspace.clone().into(), id.clone().into(), input.title.trim().into(), job.requester.clone().into(), document.clone().into(), text.clone().into(), now().into(), input.folder.clone().into()]).await?;
        execute(&tx, "INSERT INTO relay_library_versions(workspace,item,revision,author,title,document,text,created,agent,job) VALUES($1,$2,1,$3,$4,$5,$6,$7,$8,$9)", vec![job.workspace.clone().into(), id.clone().into(), job.requester.clone().into(), input.title.trim().into(), document.into(), text.into(), now().into(), job.agent.clone().into(), job.reply.clone().into()]).await?;
        super::passages::index_item(&tx, &job.workspace, &id).await?;
        let name = name_of(&tx, &job.workspace, &job.agent).await?;
        audit_with_summary(
            &tx,
            &job.requester,
            Some(&job.workspace),
            "library_page_saved_by_agent",
            &id,
            &format!(
                "{name} saved \"{}\" in \"{}\"",
                input.title.trim(),
                folder["title"].as_str().unwrap_or("")
            ),
        )
        .await?;
        tx.commit().await?;
        Ok(
            json!({"state":"saved","id":id,"link":format!("/w/{}/?view=library&item={id}",job.workspace),"next":"Share the link with the person."}),
        )
    }

    /// Suggestions waiting on a page, for people who can open it.
    pub(crate) async fn library_suggestions(
        &self,
        user: &User,
        w: &str,
        id: &str,
    ) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let page = item(&tx, user, w, id, false).await?;
        let suggestions = json_rows(&tx, "SELECT jsonb_build_object('id',x.id,'base_revision',x.base_revision,'text',x.text,'note',x.note,'created',x.created,'agent',COALESCE(a.name,'An agent'),'requester',COALESCE(u.display_name,u.email),'stale',x.base_revision<>i.revision) AS data FROM relay_library_suggestions x JOIN relay_library_items i ON i.workspace=x.workspace AND i.id=x.item LEFT JOIN relay_shared_agents a ON a.workspace=x.workspace AND a.id=x.agent LEFT JOIN relay_users u ON u.id=x.requester WHERE x.workspace=$1 AND x.item=$2 AND x.state='pending' ORDER BY x.created,x.id", vec![w.into(), id.into()]).await?;
        tx.commit().await?;
        Ok(json!({"suggestions":suggestions,"current":page["text"],"editable":page["editable"]}))
    }

    /// A person with edit access accepts (a new version, marked with the agent) or
    /// rejects a suggestion. A suggestion made on an older revision can only be rejected.
    pub(crate) async fn library_decide_suggestion(
        &self,
        user: &User,
        w: &str,
        id: &str,
        suggestion: &str,
        input: &Decision,
    ) -> Result<Value> {
        ensure!(
            ["accept", "reject"].contains(&input.action.as_str()),
            "Accept or reject the suggestion"
        );
        let tx = self.workspace_tx(w).await?;
        let page = item(&tx, user, w, id, false).await?;
        let payload = json!({"suggestion":suggestion,"action":input.action});
        if let Some(result) = retry(&tx, user, w, &input.request, &payload).await? {
            tx.commit().await?;
            return Ok(result);
        }
        ensure!(
            page["editable"] == true,
            "Only people who can edit this page decide suggestions"
        );
        let found = json_rows(&tx, "SELECT to_jsonb(x) AS data FROM relay_library_suggestions x WHERE workspace=$1 AND id=$2 AND item=$3 AND state='pending'", vec![w.into(), suggestion.into(), id.into()]).await?.into_iter().next().context("This suggestion was already decided")?;
        let title = page["title"].as_str().unwrap_or("").to_owned();
        let name = name_of(&tx, w, found["agent"].as_str().unwrap_or("")).await?;
        let result = if input.action == "accept" {
            if found["base_revision"] != page["revision"] {
                return Err(Conflict.into());
            }
            let revision = write_revision(
                &tx,
                w,
                id,
                &user.id,
                &title,
                &found["document"],
                found["text"].as_str().unwrap_or(""),
                found["agent"].as_str().unwrap_or(""),
                found["job"].as_str().unwrap_or(""),
            )
            .await?;
            audit_with_summary(
                &tx,
                &user.id,
                Some(w),
                "library_suggestion_accepted",
                id,
                &format!("Accepted {name}'s change to \"{title}\" (revision {revision})"),
            )
            .await?;
            json!({"state":"accepted","revision":revision})
        } else {
            audit_with_summary(
                &tx,
                &user.id,
                Some(w),
                "library_suggestion_rejected",
                id,
                &format!("Rejected {name}'s change to \"{title}\""),
            )
            .await?;
            json!({"state":"rejected"})
        };
        execute(&tx, "UPDATE relay_library_suggestions SET state=$3,decided_by=$4,decided=$5 WHERE workspace=$1 AND id=$2", vec![w.into(), suggestion.into(), (if input.action == "accept" { "accepted" } else { "rejected" }).into(), user.id.clone().into(), now().into()]).await?;
        remember(&tx, user, w, &input.request, payload, &result).await?;
        tx.commit().await?;
        Ok(result)
    }
}

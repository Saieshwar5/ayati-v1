//! Watched folders (LB6). A folder's manager asks an agent to handle what arrives in
//! it, with instructions ("read the invoice, record the amount and due date, file it
//! under Invoices and ask me whether to pay"). Each new item becomes one piece of
//! background work in the watcher's own conversation, for that person, so the agent
//! reaches only what they and the agent's Library access allow. Agents can file an item
//! into a folder they may save in and record a few key fields on it.
use super::edits::{name_of, standing, unavailable};
use super::*;
use crate::identity::chat::model::{Job, NewChat, NewMessage};
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use serde::Deserialize;

/// Schema 83: watched folders, arrivals and key fields.
pub(crate) struct LibraryWatching;
impl MigrationName for LibraryWatching {
    fn name(&self) -> &str {
        "m20261105_000083_library_watching"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for LibraryWatching {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("watching.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Watched folders require a forward migration".into(),
        ))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Watch {
    /// The agent to watch the folder, or none to stop.
    pub agent: Option<String>,
    #[serde(default)]
    pub instructions: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FileItem {
    pub id: String,
    pub folder: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SetFields {
    pub id: String,
    pub fields: serde_json::Map<String, Value>,
}

fn fields_checked(fields: &serde_json::Map<String, Value>) -> Result<()> {
    ensure!(fields.len() <= 20, "Record at most 20 fields");
    for (name, value) in fields {
        ensure!(
            !name.trim().is_empty() && name.chars().count() <= 40,
            "Name each field in up to 40 characters"
        );
        let fits = match value {
            Value::String(s) => s.chars().count() <= 200,
            Value::Number(_) | Value::Bool(_) | Value::Null => true,
            _ => false,
        };
        ensure!(fits, "\"{name}\" is a short text, a number, true or false");
    }
    Ok(())
}

/// An item's key fields, if any were recorded (agents read them with the document).
pub(super) async fn fields_of(db: &impl ConnectionTrait, w: &str, id: &str) -> Result<Value> {
    let present: bool = query(
        db,
        "SELECT to_regclass('relay_library_fields') IS NOT NULL AS present",
        vec![],
    )
    .await?
    .context("Schema check")?
    .try_get("", "present")?;
    if !present {
        return Ok(Value::Null);
    }
    Ok(query(
        db,
        "SELECT fields FROM relay_library_fields WHERE workspace=$1 AND item=$2",
        vec![w.into(), id.into()],
    )
    .await?
    .map(|r| r.try_get::<Value>("", "fields"))
    .transpose()?
    .unwrap_or(Value::Null))
}
/// Marks an arrival as not handled, saying why.
async fn skip(
    tx: DatabaseTransaction,
    w: &str,
    id: &str,
    folder: &str,
    detail: &str,
) -> Result<bool> {
    execute(&tx, "UPDATE relay_library_arrivals SET state='skipped',detail=$4 WHERE workspace=$1 AND item=$2 AND folder=$3 AND state='pending'", vec![w.into(), id.into(), folder.into(), detail.into()]).await?;
    tx.commit().await?;
    Ok(false)
}

impl PgStore {
    /// Who watches the folder, for anyone who can open it.
    pub(crate) async fn library_watch(&self, user: &User, w: &str, id: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        let folder = item(&tx, user, w, id, false).await?;
        let watcher = json_rows(&tx, "SELECT jsonb_build_object('agent',x.agent,'agent_name',g.name,'instructions',x.instructions,'chat',CASE WHEN x.principal=$3 THEN x.chat END,'by',(SELECT COALESCE(u.display_name,u.email) FROM relay_users u WHERE u.id=x.principal),'waiting',(SELECT count(*) FROM relay_library_arrivals a WHERE a.workspace=x.workspace AND a.folder=x.folder AND a.state='pending')) AS data FROM relay_library_watchers x JOIN relay_shared_agents g ON g.workspace=x.workspace AND g.id=x.agent WHERE x.workspace=$1 AND x.folder=$2", vec![w.into(), id.into(), user.id.clone().into()]).await?.into_iter().next();
        tx.commit().await?;
        Ok(
            json!({"watcher":watcher,"can_change":folder["manageable"] == true && folder["kind"] == "folder"}),
        )
    }

    /// The folder's manager asks an agent to watch it, or stops it. The agent must
    /// already have the folder; the work goes into a conversation of its own.
    pub(crate) async fn library_watch_set(
        &self,
        user: &User,
        w: &str,
        id: &str,
        input: &Watch,
    ) -> Result<Value> {
        let instructions = input.instructions.trim();
        let folder = {
            let tx = self.read_tx(w).await?;
            let folder = item(&tx, user, w, id, false).await?;
            tx.commit().await?;
            folder
        };
        ensure!(
            folder["kind"] == "folder" && folder["manageable"] == true,
            "Only the folder's manager sets who watches it"
        );
        let title = folder["title"].as_str().unwrap_or("").to_owned();
        let Some(agent) = &input.agent else {
            let tx = self.workspace_tx(w).await?;
            execute(
                &tx,
                "DELETE FROM relay_library_watchers WHERE workspace=$1 AND folder=$2",
                vec![w.into(), id.into()],
            )
            .await?;
            execute(&tx, "UPDATE relay_library_arrivals SET state='skipped',detail='Nobody watches this folder any more.' WHERE workspace=$1 AND folder=$2 AND state='pending'", vec![w.into(), id.into()]).await?;
            audit_with_summary(
                &tx,
                &user.id,
                Some(w),
                "library_watch",
                id,
                &format!("Stopped watching \"{title}\""),
            )
            .await?;
            tx.commit().await?;
            return Ok(json!({"ok":true}));
        };
        ensure!(
            !instructions.is_empty() && instructions.chars().count() <= 2000,
            "Say what the agent should do with new items, in up to 2,000 characters"
        );
        // Reuse this person's conversation for the same agent; otherwise start one.
        let current = {
            let tx = self.read_tx(w).await?;
            let given = query(&tx, "SELECT 1 AS found FROM relay_library_agents WHERE workspace=$1 AND item=$2 AND agent=$3", vec![w.into(), id.into(), agent.clone().into()]).await?;
            ensure!(
                given.is_some(),
                "Give the agent this folder first, from Share folder"
            );
            let row = query(&tx, "SELECT chat FROM relay_library_watchers WHERE workspace=$1 AND folder=$2 AND agent=$3 AND principal=$4", vec![w.into(), id.into(), agent.clone().into(), user.id.clone().into()]).await?;
            tx.commit().await?;
            row.map(|r| r.try_get::<String>("", "chat")).transpose()?
        };
        let chat = match current {
            Some(chat) => chat,
            None => {
                self.chat_create(
                    user,
                    w,
                    &NewChat {
                        id: uuid::Uuid::new_v4().to_string(),
                        name: format!("{title} (Library)"),
                        users: vec![],
                        agents: vec![agent.clone()],
                        kind: "group".into(),
                        description: "New items in this Library folder are handled here.".into(),
                    },
                )
                .await?
            }
        };
        let tx = self.workspace_tx(w).await?;
        execute(&tx, "INSERT INTO relay_library_watchers(workspace,folder,agent,principal,chat,instructions,set_at) VALUES($1,$2,$3,$4,$5,$6,$7) ON CONFLICT(workspace,folder) DO UPDATE SET agent=EXCLUDED.agent,principal=EXCLUDED.principal,chat=EXCLUDED.chat,instructions=EXCLUDED.instructions,set_at=EXCLUDED.set_at", vec![w.into(), id.into(), agent.clone().into(), user.id.clone().into(), chat.clone().into(), instructions.into(), now().into()]).await?;
        let name = name_of(&tx, w, agent).await?;
        audit_with_summary(
            &tx,
            &user.id,
            Some(w),
            "library_watch",
            id,
            &format!("Asked {name} to handle new items in \"{title}\""),
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"ok":true,"chat":chat}))
    }

    /// Starts agent work for items that arrived in watched folders; one at a time per
    /// folder, so a batch of uploads is handled in order.
    pub(crate) async fn dispatch_library_arrivals(&self) -> Result<usize> {
        if !self.pooled
            || query(
                &self.db,
                "SELECT to_regclass('relay_library_arrivals') IS NOT NULL AS present",
                vec![],
            )
            .await?
            .context("Schema check")?
            .try_get::<bool>("", "present")
            .ok()
                != Some(true)
        {
            return Ok(0);
        }
        let waiting = rows(&self.db, "SELECT a.workspace,a.item,a.folder FROM relay_library_arrivals a WHERE a.state='pending' AND NOT EXISTS(SELECT 1 FROM relay_library_arrivals b JOIN relay_chat_jobs j ON j.workspace=b.workspace AND j.reply=b.reply WHERE b.workspace=a.workspace AND b.folder=a.folder AND b.state='started' AND j.state<>'finished') ORDER BY a.at,a.item LIMIT 20", vec![]).await?;
        let mut started = 0;
        let mut seen = std::collections::HashSet::new();
        for row in waiting {
            let (w, item_id, folder): (String, String, String) = (
                row.try_get("", "workspace")?,
                row.try_get("", "item")?,
                row.try_get("", "folder")?,
            );
            if seen.insert((w.clone(), folder.clone())) {
                started += usize::from(self.start_arrival(&w, &item_id, &folder).await?);
            }
        }
        Ok(started)
    }

    async fn start_arrival(&self, w: &str, id: &str, folder: &str) -> Result<bool> {
        let tx = self.workspace_tx(w).await?;
        let Some(watcher) = json_rows(&tx, "SELECT to_jsonb(x) || jsonb_build_object('title',(SELECT title FROM relay_library_items WHERE workspace=$1 AND id=$2 AND folder=$3 AND NOT trashed),'folder_title',(SELECT title FROM relay_library_items WHERE workspace=$1 AND id=$3)) AS data FROM relay_library_watchers x WHERE x.workspace=$1 AND x.folder=$3", vec![w.into(), id.into(), folder.into()]).await?.into_iter().next() else {
            return skip(tx, w, id, folder, "Nobody watches this folder any more.").await;
        };
        let Some(title) = watcher["title"].as_str().map(str::to_owned) else {
            return skip(
                tx,
                w,
                id,
                folder,
                "The item left the folder before it was handled.",
            )
            .await;
        };
        let principal = query(
            &tx,
            "SELECT id,email,display_name FROM relay_users WHERE id=$1",
            vec![watcher["principal"].as_str().unwrap_or("").into()],
        )
        .await?
        .context("Watcher")?;
        let principal = User {
            id: principal.try_get("", "id")?,
            email: principal.try_get("", "email")?,
            display_name: principal.try_get("", "display_name")?,
        };
        let input = NewMessage {
            reply_to: None,
            id: uuid::Uuid::new_v4().to_string(),
            agent: watcher["agent"].as_str().map(str::to_owned),
            thread: None,
            attachments: vec![],
            content: format!(
                "New in the Library folder \"{}\": {title}\n{}\nThe new item's id is {id}. Read it with read_document, then do what these instructions say.",
                watcher["folder_title"].as_str().unwrap_or(""),
                watcher["instructions"].as_str().unwrap_or("")
            ),
        };
        let nested = tx.begin().await?;
        let job = match self
            .background_in(
                &nested,
                &principal,
                w,
                watcher["chat"].as_str().unwrap_or(""),
                &input,
            )
            .await
        {
            Ok(job) => {
                nested.commit().await?;
                job.context("The Library request was not accepted")?
            }
            Err(error) => {
                nested.rollback().await?;
                if error.is::<crate::identity::chat::model::QueueFull>()
                    || error.is::<crate::identity::chat::model::BackgroundQueueFull>()
                {
                    tx.commit().await?;
                    return Ok(false);
                }
                if error.is::<sea_orm::DbErr>() {
                    return Err(error);
                }
                let detail = if error.is::<crate::identity::chat::model::AgentsPaused>() {
                    "All agents were paused when this arrived, so it was not handled."
                } else {
                    "The watcher can no longer ask this agent; it was not handled."
                };
                return skip(tx, w, id, folder, detail).await;
            }
        };
        execute(&tx, "UPDATE relay_library_arrivals SET state='started',reply=$4 WHERE workspace=$1 AND item=$2 AND folder=$3 AND state='pending'", vec![w.into(), id.into(), folder.into(), job.reply.clone().into()]).await?;
        tx.commit().await?;
        Ok(true)
    }

    /// Agent tool `file_document`: moves an item into a folder the agent may save in,
    /// for a person who may add to it. The item then follows that folder's sharing.
    pub(crate) async fn library_file_document(&self, job: &Job, input: &FileItem) -> Result<Value> {
        let tx = self.workspace_tx(&job.workspace).await?;
        super::super::control_actions::authorize(&tx, job, true).await?;
        let (Some((_, _, current)), Some((level, access, target))) = (
            standing(&tx, job, &input.id).await?,
            standing(&tx, job, &input.folder).await?,
        ) else {
            tx.commit().await?;
            return Ok(unavailable());
        };
        if target["kind"] != "folder" || level < 3 || access.as_deref() != Some("edit") {
            tx.commit().await?;
            return Ok(
                json!({"state":"not_allowed","message":"You may file only into folders you were allowed to save in, for a person who may add to them."}),
            );
        }
        if current["kind"] == "folder"
            || super::keeping::held(&tx, &job.workspace, &input.id).await?
        {
            tx.commit().await?;
            return Ok(json!({"state":"not_allowed","message":"This item can't be moved."}));
        }
        execute(&tx, "UPDATE relay_library_items SET folder=$3,visibility='private',access_revision=access_revision+1,updated=$4 WHERE workspace=$1 AND id=$2", vec![job.workspace.clone().into(), input.id.clone().into(), input.folder.clone().into(), now().into()]).await?;
        execute(
            &tx,
            "DELETE FROM relay_library_people WHERE workspace=$1 AND item=$2",
            vec![job.workspace.clone().into(), input.id.clone().into()],
        )
        .await?;
        // The agent's own filing changed the item's access. If the people in this
        // conversation may still use it where it now is, this reply's read of it stays
        // valid; otherwise the fence keeps the answer back, as for any access change.
        execute(&tx, "UPDATE relay_library_reads r SET access_revision=i.access_revision FROM relay_library_items i WHERE r.workspace=$1 AND r.reply=$2 AND r.item=$3 AND i.workspace=r.workspace AND i.id=r.item AND relay_library_agent_use(r.workspace,r.item,$4,r.reply) IS NOT NULL", vec![job.workspace.clone().into(), job.reply.clone().into(), input.id.clone().into(), job.agent.clone().into()]).await?;
        let name = name_of(&tx, &job.workspace, &job.agent).await?;
        audit_with_summary(
            &tx,
            &job.requester,
            Some(&job.workspace),
            "library_agent_filed",
            &input.id,
            &format!(
                "{name} filed \"{}\" in \"{}\"",
                current["title"].as_str().unwrap_or(""),
                target["title"].as_str().unwrap_or("")
            ),
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"state":"filed","folder":input.folder}))
    }

    /// Agent tool `set_document_fields`: records key fields (an amount, a due date) on
    /// an item it may suggest changes to, for a person who may edit it.
    pub(crate) async fn library_set_fields(&self, job: &Job, input: &SetFields) -> Result<Value> {
        fields_checked(&input.fields)?;
        let tx = self.workspace_tx(&job.workspace).await?;
        super::super::control_actions::authorize(&tx, job, true).await?;
        let Some((level, access, current)) = standing(&tx, job, &input.id).await? else {
            tx.commit().await?;
            return Ok(unavailable());
        };
        if level < 1 || access.as_deref() != Some("edit") {
            tx.commit().await?;
            return Ok(
                json!({"state":"not_allowed","message":"You may only read this item. Tell the person the fields instead."}),
            );
        }
        execute(&tx, "INSERT INTO relay_library_fields(workspace,item,fields,agent,set_at) VALUES($1,$2,$3,$4,$5) ON CONFLICT(workspace,item) DO UPDATE SET fields=relay_library_fields.fields || EXCLUDED.fields,agent=EXCLUDED.agent,set_at=EXCLUDED.set_at", vec![job.workspace.clone().into(), input.id.clone().into(), Value::Object(input.fields.clone()).into(), job.agent.clone().into(), now().into()]).await?;
        let name = name_of(&tx, &job.workspace, &job.agent).await?;
        audit_with_summary(
            &tx,
            &job.requester,
            Some(&job.workspace),
            "library_agent_fields",
            &input.id,
            &format!(
                "{name} recorded {} on \"{}\"",
                input.fields.keys().cloned().collect::<Vec<_>>().join(", "),
                current["title"].as_str().unwrap_or("")
            ),
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"state":"saved"}))
    }

    /// An item's key fields, for anyone who can open it.
    pub(crate) async fn library_fields(&self, user: &User, w: &str, id: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        item(&tx, user, w, id, false).await?;
        let fields = json_rows(&tx, "SELECT jsonb_build_object('fields',f.fields,'agent',(SELECT name FROM relay_shared_agents g WHERE g.workspace=f.workspace AND g.id=f.agent),'at',f.set_at) AS data FROM relay_library_fields f WHERE f.workspace=$1 AND f.item=$2", vec![w.into(), id.into()]).await?.into_iter().next();
        tx.commit().await?;
        Ok(json!({"fields":fields}))
    }
}

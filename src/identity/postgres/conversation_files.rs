//! Files of a conversation (schema 159): what people attached and what an agent made
//! there. They belong to the conversation and are seen by the people in it; the Library
//! holds only what someone chose to keep. An agent lists them, opens one on its computer
//! and attaches what it makes to its reply; a person saves one to the Library.
use super::computer_tools::{resolve, shown};
use super::*;
use crate::identity::chat::model::Job;
use crate::identity::collaboration::Object;
use crate::identity::library::model::{MAX_FILE, checksum, title, uuid};
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use serde::Deserialize;
use serde_json::json;

pub(crate) struct ConversationFiles;
impl MigrationName for ConversationFiles {
    fn name(&self) -> &str {
        "m20261126_000159_conversation_files"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for ConversationFiles {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("conversation_files.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Conversation files require a forward migration".into(),
        ))
    }
}

/// Where file bytes are kept: the Library's storage, and Chat's files, which are in that
/// same storage when hosted and under each workspace's local folder otherwise.
#[derive(Clone, Default)]
pub(crate) struct Files {
    pub library: crate::identity::library::Storage,
    pub chat_root: Option<std::path::PathBuf>,
    pub hosted: bool,
}
impl Files {
    pub(crate) async fn chat_get(&self, w: &str, id: &str) -> Result<axum::body::Bytes> {
        if self.hosted {
            return self.library.get(&format!("chat/{w}/{id}")).await;
        }
        let root = self
            .chat_root
            .as_ref()
            .context("Chat files are not set up")?;
        Ok(tokio::fs::read(root.join(w).join("objects").join(id))
            .await
            .context("Stored file unavailable")?
            .into())
    }
    pub(crate) async fn chat_put(&self, w: &str, id: &str, bytes: axum::body::Bytes) -> Result<()> {
        if self.hosted {
            return self.library.put(&format!("chat/{w}/{id}"), bytes).await;
        }
        let root = self
            .chat_root
            .as_ref()
            .context("Chat files are not set up")?;
        let folder = root.join(w).join("objects");
        tokio::fs::create_dir_all(&folder).await?;
        tokio::fs::write(folder.join(id), &bytes).await?;
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct OpenAttachment {
    id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KeepFile {
    /// The new Library item, and the request, so a retry is the same item.
    pub id: String,
    pub folder: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
}

/// A conversation's files, newest first: attached to a message, or made by an agent.
const LISTED: &str = "jsonb_build_object('id',o.id,'name',o.name,'mime',o.mime,'size',o.size,'at',o.created,'chat',o.chat,'chat_name',c.name,'library_item',o.library_item,'made_by',(SELECT a.name FROM relay_shared_agents a WHERE a.workspace=o.workspace AND a.id=o.made_by),'from',(SELECT COALESCE(u.display_name,u.email) FROM relay_users u WHERE u.id=o.uploader)) AS data FROM relay_objects o JOIN relay_chats c ON c.workspace=o.workspace AND c.id=o.chat WHERE o.workspace=$1 AND EXISTS(SELECT 1 FROM relay_chat_messages m WHERE m.workspace=o.workspace AND m.chat=o.chat AND m.attachments @> jsonb_build_array(o.id) AND m.state<>'deleted')";

fn file_name(name: &str) -> String {
    let safe: String = name
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
    if safe.trim_matches(['.', ' ', '_']).is_empty() {
        "file".into()
    } else {
        safe
    }
}

impl PgStore {
    /// The files of a conversation, for the people in it.
    pub(crate) async fn chat_files(&self, user: &User, w: &str, chat: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        super::chat::access(&tx, user, w, chat).await?;
        let files = json_rows(
            &tx,
            &format!("SELECT {LISTED} AND o.chat=$2 ORDER BY o.created DESC,o.id LIMIT 200"),
            vec![w.into(), chat.into()],
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"files":files}))
    }

    /// Files sent to an agent or made by it, across its conversations. A person sees
    /// the conversations they are in; the agent's owners and admins see the rest too.
    pub(crate) async fn agent_artifacts(&self, user: &User, w: &str, agent: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        super::agent_profiles::managed_access(&tx, &user.id, w, agent, false).await?;
        let keeper = role(&tx, w, &user.id).await? == "Admin"
            || query(&tx, "SELECT 1 AS found FROM relay_agent_owners WHERE workspace=$1 AND agent=$2 AND user_id=$3", vec![w.into(), agent.into(), user.id.clone().into()]).await?.is_some();
        let files = json_rows(&tx, &format!("SELECT {LISTED} AND (o.made_by=$2 OR EXISTS(SELECT 1 FROM relay_chat_messages m WHERE m.workspace=o.workspace AND m.chat=o.chat AND m.attachments @> jsonb_build_array(o.id) AND m.target=$2)) AND ($4 OR c.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=o.workspace AND p.chat=o.chat AND p.user_id=$3)) ORDER BY o.created DESC,o.id LIMIT 200"), vec![w.into(), agent.into(), user.id.clone().into(), keeper.into()]).await?
            .into_iter()
            .map(|mut f| {
                f["mine"] = json!(true);
                f
            })
            .collect::<Vec<_>>();
        let mine: Vec<String> = rows(&tx, "SELECT c.id FROM relay_chats c WHERE c.workspace=$1 AND (c.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id=$2))", vec![w.into(), user.id.clone().into()]).await?.into_iter().filter_map(|r| r.try_get("", "id").ok()).collect();
        tx.commit().await?;
        // Mark what the person sees only as the agent's keeper.
        let files: Vec<Value> = files
            .into_iter()
            .map(|mut f| {
                f["mine"] = json!(mine.iter().any(|c| f["chat"] == json!(c)));
                f
            })
            .collect();
        Ok(json!({"files":files,"keeper":keeper}))
    }

    /// A person keeps a conversation's file in the Library: a copy, as a new file in a
    /// folder they can add to (or unfiled and private).
    pub(crate) async fn chat_file_keep(
        &self,
        user: &User,
        w: &str,
        object: &str,
        input: &KeepFile,
    ) -> Result<Value> {
        uuid(&input.id)?;
        let file = self.object(user, w, object).await?;
        let name = input.title.clone().unwrap_or_else(|| file.name.clone());
        title(&name)?;
        if let Some(kept) = query(&self.db, "SELECT library_item FROM relay_objects WHERE workspace=$1 AND id=$2 AND library_item=$3", vec![w.into(), object.into(), input.id.clone().into()]).await? {
            return Ok(json!({"id":kept.try_get::<String>("", "library_item")?}));
        }
        let bytes = self.files.chat_get(w, object).await?;
        ensure!(
            self.files.library.enabled(),
            "File storage is not configured. Download the file instead."
        );
        ensure!(
            super::library::file_versions::storage_used(&self.db, w).await? + file.size
                <= crate::identity::library::model::WORKSPACE_QUOTA,
            "Library storage is full"
        );
        let key = format!("library/{w}/{}", input.id);
        self.files.library.put(&key, bytes.clone()).await?;
        let mime = crate::identity::library::file_mime(&bytes);
        let (text, readiness) = crate::identity::library::file_text(&bytes, mime).await;
        let tx = self.workspace_tx(w).await?;
        if let Some(folder) = &input.folder {
            super::library::folders::open_folder(&tx, user, w, folder).await?;
        }
        execute(&tx, "INSERT INTO relay_library_items(workspace,id,kind,title,created_by,steward,text,object_key,mime,size,checksum,readiness,created,updated,folder) VALUES($1,$2,'file',$3,$4,$4,$5,$6,$7,$8,$9,$10,$11,$11,$12)", vec![w.into(), input.id.clone().into(), name.trim().into(), user.id.clone().into(), text.into(), key.into(), mime.into(), file.size.into(), file.checksum.clone().into(), readiness.into(), now().into(), input.folder.clone().into()]).await?;
        super::library::file_versions::snapshot(&tx, w, &input.id, &user.id, None).await?;
        super::library::passages::index_item(&tx, w, &input.id).await?;
        execute(
            &tx,
            "UPDATE relay_objects SET library_item=$3 WHERE workspace=$1 AND id=$2",
            vec![w.into(), object.into(), input.id.clone().into()],
        )
        .await?;
        audit_with_summary(
            &tx,
            &user.id,
            Some(w),
            "library_file_kept",
            &input.id,
            &format!(
                "Saved \"{}\" from a conversation to the Library",
                name.trim()
            ),
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"id":input.id}))
    }

    /// The agent lists the files of the conversation it is working in.
    pub(crate) async fn agent_conversation_files(&self, job: &Job) -> Result<Value> {
        let tx = self.workspace_tx(&job.workspace).await?;
        super::control_actions::authorize(&tx, job, true).await?;
        let files = json_rows(
            &tx,
            &format!("SELECT {LISTED} AND o.chat=$2 ORDER BY o.created DESC,o.id LIMIT 100"),
            vec![job.workspace.clone().into(), job.chat.clone().into()],
        )
        .await?;
        tx.commit().await?;
        Ok(
            json!({"state":"available","files":files,"next":"Open one on your computer with open_attachment. A file you make is attached to your reply with save_file; name a Library folder only when the person asks to keep it there."}),
        )
    }

    /// Puts one of the conversation's files on the agent's computer.
    pub(crate) async fn agent_open_attachment(
        &self,
        job: &Job,
        input: &OpenAttachment,
    ) -> Result<Value> {
        let tx = self.workspace_tx(&job.workspace).await?;
        super::control_actions::authorize(&tx, job, true).await?;
        let found = json_rows(
            &tx,
            &format!("SELECT {LISTED} AND o.chat=$2 AND o.id=$3"),
            vec![
                job.workspace.clone().into(),
                job.chat.clone().into(),
                input.id.clone().into(),
            ],
        )
        .await?
        .into_iter()
        .next();
        tx.commit().await?;
        let Some(found) = found else {
            return Ok(
                json!({"state":"not_found","message":"No such file in this conversation. List them with conversation_files."}),
            );
        };
        let Ok(bytes) = self.files.chat_get(&job.workspace, &input.id).await else {
            return Ok(
                json!({"state":"unavailable","message":"This file's stored copy can't be reached. Tell the person."}),
            );
        };
        let ready = match self.computer_ready(job).await? {
            Ok(ready) => ready,
            Err(refusal) => return Ok(refusal),
        };
        let relative = format!(
            "attachments/{}",
            file_name(found["name"].as_str().unwrap_or(""))
        );
        let path = match resolve(job, &relative) {
            Ok(path) => path,
            Err(refusal) => return Ok(refusal),
        };
        let size = bytes.len();
        self.computer_put(&ready, &path, bytes.to_vec()).await?;
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
        Ok(json!({"state":"opened","path":relative,"name":found["name"],"bytes":size}))
    }

    /// Attaches a file the agent made to its reply: it becomes one of the conversation's
    /// files, for the people in it.
    pub(crate) async fn agent_attach_file(
        &self,
        job: &Job,
        name: &str,
        bytes: Vec<u8>,
    ) -> Result<Value> {
        if bytes.is_empty() || bytes.len() > MAX_FILE {
            return Ok(
                json!({"state":"invalid","message":"Attach a file between 1 byte and 25 MiB."}),
            );
        }
        let name = file_name(name);
        let tx = self.workspace_tx(&job.workspace).await?;
        super::control_actions::authorize(&tx, job, true).await?;
        let attached: i64 = query(&tx, "SELECT jsonb_array_length(attachments)::bigint AS n FROM relay_chat_messages WHERE workspace=$1 AND id=$2", vec![job.workspace.clone().into(), job.reply.clone().into()]).await?.context("Reply unavailable")?.try_get("", "n")?;
        tx.commit().await?;
        if attached >= 8 {
            return Ok(
                json!({"state":"invalid","message":"A reply carries at most eight files. Put the rest in one archive or tell the person."}),
            );
        }
        let bytes = axum::body::Bytes::from(bytes);
        let object = Object {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            mime: crate::identity::library::file_mime(&bytes).into(),
            size: bytes.len() as i64,
            checksum: checksum(&bytes),
        };
        self.files
            .chat_put(&job.workspace, &object.id, bytes)
            .await?;
        let tx = self.workspace_tx(&job.workspace).await?;
        super::control_actions::authorize(&tx, job, true).await?;
        execute(&tx, "INSERT INTO relay_objects(workspace,chat,id,uploader,name,mime,size,checksum,created,made_by) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)", vec![job.workspace.clone().into(), job.chat.clone().into(), object.id.clone().into(), job.requester.clone().into(), object.name.clone().into(), object.mime.clone().into(), object.size.into(), object.checksum.clone().into(), now().into(), job.agent.clone().into()]).await?;
        execute(&tx, "UPDATE relay_chat_messages SET attachments=attachments || jsonb_build_array($3::text) WHERE workspace=$1 AND id=$2", vec![job.workspace.clone().into(), job.reply.clone().into(), object.id.clone().into()]).await?;
        tx.commit().await?;
        Ok(
            json!({"state":"attached","file":object.id,"name":object.name,"next":"It is attached to your reply and listed in the conversation's files. Say in one line what it is."}),
        )
    }
}

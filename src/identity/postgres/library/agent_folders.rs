//! Each agent's own folder (schema 155): one subfolder per conversation, where what the
//! agent makes for that conversation is kept. A conversation's subfolder follows who is
//! in the conversation, so someone else asking the same agent never sees it; the agent's
//! owners and admins see everything. The folders are made when first needed and can't
//! be shared, moved or trashed by hand.
use super::*;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};

pub(crate) struct LibraryAgentFolders;
impl MigrationName for LibraryAgentFolders {
    fn name(&self) -> &str {
        "m20261125_000155_library_agent_folders"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for LibraryAgentFolders {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("agent_folders.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Agents' Library folders require a forward migration".into(),
        ))
    }
}

async fn present(db: &impl ConnectionTrait) -> Result<bool> {
    Ok(query(
        db,
        "SELECT to_regclass('relay_library_agent_folders') IS NOT NULL AS present",
        vec![],
    )
    .await?
    .context("Schema check")?
    .try_get("", "present")?)
}

/// Refuses changing an agent's folder by hand: its place and sharing follow the agent
/// and the conversation.
pub(super) async fn not_agents(db: &impl ConnectionTrait, w: &str, id: &str) -> Result<()> {
    if !present(db).await? {
        return Ok(());
    }
    ensure!(
        query(
            db,
            "SELECT 1 AS found FROM relay_library_agent_folders WHERE workspace=$1 AND folder=$2",
            vec![w.into(), id.into()]
        )
        .await?
        .is_none(),
        "This is an agent's folder. It follows the agent and its conversations; it can't be shared, moved or trashed."
    );
    Ok(())
}

/// One of an agent's folders, made now if it isn't there: its top folder, or under it
/// the subfolder for a conversation. `by` is the person the agent is working for.
async fn folder(
    tx: &DatabaseTransaction,
    w: &str,
    agent: &str,
    chat: &str,
    parent: Option<&str>,
    by: &str,
) -> Result<String> {
    if let Some(row) = query(tx, "SELECT folder FROM relay_library_agent_folders WHERE workspace=$1 AND agent=$2 AND chat=$3", vec![w.into(), agent.into(), chat.into()]).await? {
        return Ok(row.try_get("", "folder")?);
    }
    let named = if chat.is_empty() {
        query(
            tx,
            "SELECT name FROM relay_shared_agents WHERE workspace=$1 AND id=$2",
            vec![w.into(), agent.into()],
        )
        .await?
    } else {
        query(
            tx,
            "SELECT name FROM relay_chats WHERE workspace=$1 AND id=$2",
            vec![w.into(), chat.into()],
        )
        .await?
    };
    let name: String = named
        .context("Agent or conversation unavailable")?
        .try_get("", "name")?;
    let name: String = name.chars().filter(|c| !c.is_control()).take(120).collect();
    let name = if name.trim().is_empty() {
        "Conversation".into()
    } else {
        name
    };
    let id = uuid::Uuid::new_v4().to_string();
    // No steward: nobody keeps it privately. A conversation's subfolder has its own
    // sharing, with the people in the conversation: it lists as shared for them.
    execute(tx, "INSERT INTO relay_library_items(workspace,id,kind,title,created_by,created,updated,folder,shares,visibility) VALUES($1,$2,'folder',$3,$4,$5,$5,$6,$7,CASE WHEN $7 THEN 'selected' ELSE 'private' END)", vec![w.into(), id.clone().into(), name.trim().into(), by.into(), now().into(), parent.map(str::to_owned).into(), parent.is_some().into()]).await?;
    execute(tx, "INSERT INTO relay_library_agents(workspace,item,agent,level,use_for) VALUES($1,$2,$3,'save','viewers')", vec![w.into(), id.clone().into(), agent.into()]).await?;
    execute(
        tx,
        "INSERT INTO relay_library_agent_folders(workspace,agent,chat,folder) VALUES($1,$2,$3,$4)",
        vec![w.into(), agent.into(), chat.into(), id.clone().into()],
    )
    .await?;
    Ok(id)
}

/// The agent's subfolder for a conversation, where new files go when no folder is named.
pub(super) async fn for_conversation(
    tx: &DatabaseTransaction,
    w: &str,
    agent: &str,
    chat: &str,
    by: &str,
) -> Result<String> {
    let top = folder(tx, w, agent, "", None, by).await?;
    folder(tx, w, agent, chat, Some(&top), by).await
}

/// Files a person attached when asking an agent are kept in the agent's folder for that
/// conversation (C8), so the agent can open them on its computer. The item names the
/// chat file's own stored bytes: nothing is stored twice, and Chat keeps owning them.
pub(in crate::identity::postgres) async fn keep_attachments(
    tx: &DatabaseTransaction,
    w: &str,
    chat: &str,
    by: &str,
    agent: &str,
    attachments: &[String],
) -> Result<()> {
    if attachments.is_empty() || !present(tx).await? {
        return Ok(());
    }
    let place = for_conversation(tx, w, agent, chat, by).await?;
    for object in attachments {
        let key = format!("chat/{w}/{object}");
        let count: i64 = query(
            tx,
            "SELECT count(*) AS n FROM relay_library_items WHERE workspace=$1",
            vec![w.into()],
        )
        .await?
        .context("Count")?
        .try_get("", "n")?;
        // A full Library never stops a message; the file stays in Chat only.
        if count >= MAX_ITEMS {
            return Ok(());
        }
        let id = uuid::Uuid::new_v4().to_string();
        let made = execute(tx, "INSERT INTO relay_library_items(workspace,id,kind,title,created_by,text,object_key,mime,size,checksum,readiness,created,updated,folder) SELECT o.workspace,$3,'file',o.name,$4,'',$5,o.mime,o.size,o.checksum,'unsupported',$6,$6,$7 FROM relay_objects o WHERE o.workspace=$1 AND o.id=$2 AND NOT EXISTS(SELECT 1 FROM relay_library_items i WHERE i.workspace=o.workspace AND i.folder=$7 AND i.object_key=$5)", vec![w.into(), object.clone().into(), id.clone().into(), by.into(), key.into(), now().into(), place.clone().into()]).await?;
        if made == 1 {
            file_versions::snapshot(tx, w, &id, by, None).await?;
        }
    }
    Ok(())
}

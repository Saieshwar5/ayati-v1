//! Editing and deleting your own messages, and reacting to any message you can
//! read. Reactions are expressions only: they never approve or request work.
use super::*;

/// The reactions offered in Chat. A small fixed set keeps them readable.
pub const REACTIONS: [&str; 12] = [
    "👍", "✅", "👀", "🙏", "🎉", "❤️", "😂", "🔥", "👏", "🤔", "😮", "💯",
];

impl PgStore {
    /// Changes the text of the caller's own message. Agent work it already
    /// started continues with the original request.
    pub async fn edit_message(
        &self,
        user: &User,
        w: &str,
        chat: &str,
        message: &str,
        content: &str,
    ) -> Result<()> {
        ensure!(
            !content.trim().is_empty() && content.len() <= 12000,
            "Write a message of at most 12,000 bytes"
        );
        let tx = self.workspace_tx(w).await?;
        own_open_message(&tx, user, w, chat, message).await?;
        execute(
            &tx,
            "UPDATE relay_chat_messages SET content=$3,edited_at=$4 WHERE workspace=$1 AND id=$2",
            vec![w.into(), message.into(), content.into(), now().into()],
        )
        .await?;
        audit(&tx, &user.id, Some(w), "message_edited", message).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Removes the text and files of the caller's own message and stops any
    /// agent work it asked for that has not finished. The message keeps its
    /// place so thread replies stay attached.
    pub async fn delete_message(
        &self,
        user: &User,
        w: &str,
        chat: &str,
        message: &str,
    ) -> Result<u64> {
        let tx = self.workspace_tx(w).await?;
        own_open_message(&tx, user, w, chat, message).await?;
        execute(
            &tx,
            "UPDATE relay_chat_messages SET content='',attachments='[]',state='deleted',edited_at=$3 WHERE workspace=$1 AND id=$2",
            vec![w.into(), message.into(), now().into()],
        )
        .await?;
        execute(
            &tx,
            "DELETE FROM relay_message_reactions WHERE workspace=$1 AND message=$2",
            vec![w.into(), message.into()],
        )
        .await?;
        let replies = rows(
            &tx,
            "SELECT reply FROM relay_chat_jobs WHERE workspace=$1 AND request_message=$2 AND state<>'finished'",
            vec![w.into(), message.into()],
        )
        .await?;
        let mut stopped = 0;
        for reply in replies {
            let reply: String = reply.try_get("", "reply")?;
            if super::assignments::stop_reply(&tx, &user.id, w, &reply).await? {
                stopped += 1;
            }
        }
        audit(&tx, &user.id, Some(w), "message_deleted", message).await?;
        tx.commit().await?;
        Ok(stopped)
    }

    /// Adds or removes one of the caller's reactions. Repeating either is harmless.
    pub async fn react(
        &self,
        user: &User,
        w: &str,
        chat: &str,
        message: &str,
        emoji: &str,
        on: bool,
    ) -> Result<()> {
        ensure!(
            REACTIONS.contains(&emoji),
            "Choose one of the offered reactions"
        );
        let tx = self.workspace_tx(w).await?;
        super::chat::access(&tx, user, w, chat).await?;
        let row = query(
            &tx,
            "SELECT m.state,c.archived FROM relay_chat_messages m JOIN relay_chats c ON c.workspace=m.workspace AND c.id=m.chat
             WHERE m.workspace=$1 AND m.chat=$2 AND m.id=$3",
            vec![w.into(), chat.into(), message.into()],
        )
        .await?
        .context("Message unavailable")?;
        ensure!(
            !row.try_get::<bool>("", "archived")?,
            "This channel is archived. Restore it before reacting."
        );
        ensure!(
            ["sent", "complete"].contains(&row.try_get::<String>("", "state")?.as_str()),
            "Only delivered messages can have reactions"
        );
        if on {
            execute(
                &tx,
                "INSERT INTO relay_message_reactions(workspace,chat,message,user_id,emoji,at) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING",
                vec![w.into(), chat.into(), message.into(), user.id.clone().into(), emoji.into(), now().into()],
            )
            .await?;
        } else {
            execute(
                &tx,
                "DELETE FROM relay_message_reactions WHERE workspace=$1 AND message=$2 AND user_id=$3 AND emoji=$4",
                vec![w.into(), message.into(), user.id.clone().into(), emoji.into()],
            )
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

/// Only the person who wrote a message may change it, while it is still a
/// delivered message in a conversation they can read and write.
async fn own_open_message(
    tx: &DatabaseTransaction,
    user: &User,
    w: &str,
    chat: &str,
    message: &str,
) -> Result<()> {
    super::chat::access(tx, user, w, chat).await?;
    let row = query(
        tx,
        "SELECT m.author,m.kind,m.state,c.archived FROM relay_chat_messages m JOIN relay_chats c ON c.workspace=m.workspace AND c.id=m.chat
         WHERE m.workspace=$1 AND m.chat=$2 AND m.id=$3 FOR UPDATE OF m",
        vec![w.into(), chat.into(), message.into()],
    )
    .await?
    .context("Message unavailable")?;
    ensure!(
        row.try_get::<String>("", "kind")? == "user"
            && row.try_get::<String>("", "author")? == user.id,
        "You can only change your own messages"
    );
    ensure!(
        row.try_get::<String>("", "state")? == "sent",
        "This message was deleted"
    );
    ensure!(
        !row.try_get::<bool>("", "archived")?,
        "This channel is archived. Restore it before changing messages."
    );
    Ok(())
}

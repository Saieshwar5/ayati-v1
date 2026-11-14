//! Bots in a channel (schema 119): the channel's creator or an admin adds
//! one and gets its webhook address once; anything that can post to a Slack
//! incoming webhook then posts there as that bot. A bot's message is an
//! ordinary sent message of kind `bot`: it asks no agent, starts no work and
//! approves nothing. People reply, quote it or ask an agent about it.
use super::*;
use crate::identity::provider::{digest, random_token};
use serde_json::json;

/// Bots per channel, and messages one bot may post in a minute.
const PER_CHANNEL: i64 = 10;
const PER_MINUTE: i64 = 30;

#[derive(Debug, PartialEq)]
pub(crate) enum Posted {
    Sent(String),
    /// Unknown, removed or wrong secret: the caller learns nothing more.
    Unknown,
    Archived,
    Limited,
}

impl PgStore {
    /// Adds a bot to a channel. The token is returned only here.
    pub(crate) async fn create_bot(
        &self,
        user: &User,
        w: &str,
        chat: &str,
        name: &str,
    ) -> Result<(Value, String)> {
        let name = name.trim();
        ensure!(
            (1..=60).contains(&name.chars().count()),
            "Name the bot in 1 to 60 characters"
        );
        let tx = self.workspace_tx(w).await?;
        super::collaboration::managed_channel(&tx, user, w, chat).await?;
        let count: i64 = query(
            &tx,
            "SELECT count(*) AS n FROM relay_chat_bots WHERE workspace=$1 AND chat=$2 AND enabled",
            vec![w.into(), chat.into()],
        )
        .await?
        .context("Missing count")?
        .try_get("", "n")?;
        ensure!(
            count < PER_CHANNEL,
            "A channel can have at most {PER_CHANNEL} bots"
        );
        let (id, token) = (uuid::Uuid::new_v4().to_string(), random_token());
        execute(&tx, "INSERT INTO relay_chat_bots(workspace,id,chat,name,token_hash,created_by,created) VALUES($1,$2,$3,$4,$5,$6,$7)",
            vec![w.into(), id.clone().into(), chat.into(), name.into(), digest(&token).into(), user.id.clone().into(), now().into()]).await?;
        audit(&tx, &user.id, Some(w), "chat_bot_created", &id).await?;
        let bot = view(&tx, w, &id).await?;
        tx.commit().await?;
        Ok((bot, token))
    }

    /// The channel's bots, for the people who manage it. Never their secrets.
    pub(crate) async fn chat_bots(&self, user: &User, w: &str, chat: &str) -> Result<Value> {
        let tx = self.read_tx(w).await?;
        super::collaboration::managed_channel(&tx, user, w, chat).await?;
        let bots = json_rows(
            &tx,
            &format!("{BOT} WHERE b.workspace=$1 AND b.chat=$2 AND b.enabled ORDER BY b.created"),
            vec![w.into(), chat.into()],
        )
        .await?;
        tx.commit().await?;
        Ok(json!({ "bots": bots }))
    }

    /// Removes a bot: its address stops working at once. Its messages stay.
    pub(crate) async fn remove_bot(
        &self,
        user: &User,
        w: &str,
        chat: &str,
        id: &str,
    ) -> Result<()> {
        let tx = self.workspace_tx(w).await?;
        super::collaboration::managed_channel(&tx, user, w, chat).await?;
        let changed = execute(&tx, "UPDATE relay_chat_bots SET enabled=FALSE WHERE workspace=$1 AND chat=$2 AND id=$3 AND enabled",
            vec![w.into(), chat.into(), id.into()]).await?;
        ensure!(changed == 1, "Bot unavailable");
        audit(&tx, &user.id, Some(w), "chat_bot_removed", id).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Posts `content` (already safe Markdown, `chat/bots.rs`) as the bot.
    pub(crate) async fn bot_post(&self, bot: &str, token: &str, content: &str) -> Result<Posted> {
        let Some(found) = query(
            &self.db,
            "SELECT workspace,chat FROM relay_chat_bots WHERE id=$1 AND token_hash=$2 AND enabled",
            vec![bot.into(), digest(token).into()],
        )
        .await?
        else {
            return Ok(Posted::Unknown);
        };
        let (w, chat): (String, String) =
            (found.try_get("", "workspace")?, found.try_get("", "chat")?);
        let tx = self.workspace_tx(&w).await?;
        let archived: bool = query(
            &tx,
            "SELECT archived FROM relay_chats WHERE workspace=$1 AND id=$2",
            vec![w.clone().into(), chat.clone().into()],
        )
        .await?
        .context("Conversation missing")?
        .try_get("", "archived")?;
        if archived {
            return Ok(Posted::Archived);
        }
        let recent: i64 = query(&tx, "SELECT count(*) AS n FROM relay_chat_messages WHERE workspace=$1 AND chat=$2 AND author=$3 AND kind='bot' AND at>$4",
            vec![w.clone().into(), chat.clone().into(), bot.into(), (now() - 60).into()]).await?
            .context("Missing count")?
            .try_get("", "n")?;
        if recent >= PER_MINUTE {
            return Ok(Posted::Limited);
        }
        let id = uuid::Uuid::new_v4().to_string();
        execute(&tx, "INSERT INTO relay_chat_messages(id,workspace,chat,author,kind,content,state,at) VALUES($1,$2,$3,$4,'bot',$5,'sent',$6)",
            vec![id.clone().into(), w.into(), chat.into(), bot.into(), content.into(), now().into()]).await?;
        tx.commit().await?;
        Ok(Posted::Sent(id))
    }
}

const BOT: &str = "SELECT jsonb_build_object('id',b.id,'name',b.name,'created',b.created,
    'last_post',(SELECT max(m.at) FROM relay_chat_messages m WHERE m.workspace=b.workspace AND m.chat=b.chat AND m.author=b.id AND m.kind='bot'),
    'posts_30_days',(SELECT count(*) FROM relay_chat_messages m WHERE m.workspace=b.workspace AND m.chat=b.chat AND m.author=b.id AND m.kind='bot' AND m.at>extract(epoch FROM now())::bigint-2592000)) AS data
    FROM relay_chat_bots b";

async fn view(tx: &DatabaseTransaction, w: &str, id: &str) -> Result<Value> {
    json_rows(
        tx,
        &format!("{BOT} WHERE b.workspace=$1 AND b.id=$2"),
        vec![w.into(), id.into()],
    )
    .await?
    .into_iter()
    .next()
    .context("Bot unavailable")
}

/// Gives bot messages their bot's name, read only when there are any, so
/// the shared message queries work on schemas before 119. `field` is where
/// the name goes: `name` on Chat payloads, `author.display_name` in context.
pub(super) async fn name_bots(
    db: &impl ConnectionTrait,
    w: &str,
    messages: &mut [Value],
    context: bool,
) -> Result<()> {
    let kind = |m: &Value| {
        if context {
            m["author"]["kind"].clone()
        } else {
            m["kind"].clone()
        }
    };
    let id = |m: &Value| {
        if context {
            m["author"]["id"].as_str().map(str::to_owned)
        } else {
            m["author"].as_str().map(str::to_owned)
        }
    };
    let ids: Vec<String> = messages
        .iter()
        .filter(|m| kind(m) == "bot")
        .filter_map(id)
        .collect();
    if ids.is_empty() {
        return Ok(());
    }
    let names: std::collections::HashMap<String, String> = rows(
        db,
        "SELECT id,name FROM relay_chat_bots WHERE workspace=$1 AND id=ANY($2)",
        vec![w.into(), ids.into()],
    )
    .await?
    .into_iter()
    .map(|row| {
        Ok((
            row.try_get::<String>("", "id")?,
            row.try_get::<String>("", "name")?,
        ))
    })
    .collect::<Result<_>>()?;
    for message in messages.iter_mut().filter(|m| kind(m) == "bot") {
        let name = id(message)
            .and_then(|id| names.get(&id).cloned())
            .unwrap_or_else(|| "Bot".into());
        if context {
            // Agents read it as a bot, not a person: untrusted like any message.
            message["author"]["display_name"] = json!(format!("{name} (bot)"));
        } else {
            message["name"] = json!(name);
        }
    }
    Ok(())
}

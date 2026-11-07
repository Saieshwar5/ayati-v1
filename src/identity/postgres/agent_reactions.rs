//! An emoji-only message asks for nothing (D246): instead of a model run, each
//! agent it asks reacts to it, shown as "Atlas reacted 👍". The reaction costs no
//! model call, budget or executor slot; the message is stored as usual and gets
//! the agent's delivered and seen ticks. Agents never react otherwise, and a
//! reaction, a person's or an agent's, never starts, approves or cancels work.
use super::{super::*, REACTIONS};
use crate::identity::chat::{mentions, status::single_emoji};
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use std::collections::BTreeSet;
use unicode_segmentation::UnicodeSegmentation;

/// More emoji than this read as a message of their own, which the agent answers.
const MAX_EMOJI: usize = 8;

/// The reaction an agent adds instead of answering, when `content` is only
/// emoji: apart from whitespace and @mentions, one to eight grapheme clusters
/// that are each a single emoji. It is the message's first emoji when that is
/// one of the twelve reactions (skin tone and presentation marks aside),
/// otherwise 👍. Anything else, even one letter or "?", is for the agent to read.
pub(in crate::identity::postgres) fn reaction(content: &str) -> Option<&'static str> {
    let text = mentions::without(content);
    let mut emoji = Vec::new();
    for cluster in text.graphemes(true) {
        if cluster.chars().all(char::is_whitespace) {
            continue;
        }
        if emoji.len() == MAX_EMOJI || !single_emoji(cluster) {
            return None;
        }
        emoji.push(cluster);
    }
    let bare = |emoji: &str| -> String {
        emoji
            .chars()
            .filter(|c| *c != '\u{FE0F}' && !('\u{1F3FB}'..='\u{1F3FF}').contains(c))
            .collect()
    };
    let first = bare(emoji.first()?);
    Some(
        REACTIONS
            .iter()
            .find(|reaction| bare(reaction) == first)
            .copied()
            .unwrap_or("👍"),
    )
}

/// `agent` reacts to the person's `message` and has read it (delivered and seen).
pub(in crate::identity::postgres) async fn acknowledge(
    tx: &DatabaseTransaction,
    w: &str,
    chat: &str,
    message: &str,
    agent: &str,
    emoji: &str,
) -> Result<()> {
    execute(
        tx,
        "INSERT INTO relay_agent_reactions(workspace,chat,message,agent,emoji,at) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING",
        vec![w.into(), chat.into(), message.into(), agent.into(), emoji.into(), now().into()],
    )
    .await?;
    super::super::agent_receipts::seen(tx, w, chat, message, agent).await
}

/// The agents that reacted to `message`, for an exact replay of it.
pub(in crate::identity::postgres) async fn acknowledged(
    tx: &DatabaseTransaction,
    w: &str,
    message: &str,
) -> Result<BTreeSet<String>> {
    rows(
        tx,
        "SELECT agent FROM relay_agent_reactions WHERE workspace=$1 AND message=$2",
        vec![w.into(), message.into()],
    )
    .await?
    .into_iter()
    .map(|row| -> Result<String> { Ok(row.try_get("", "agent")?) })
    .collect()
}

/// Schema 101 (D246): agents' reactions, beside people's.
pub(in crate::identity::postgres) struct AgentReactions;
impl MigrationName for AgentReactions {
    fn name(&self) -> &str {
        "m20261107_000101_agent_reactions"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for AgentReactions {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        m.get_connection()
            .execute_unprepared(include_str!("agent_reactions.sql"))
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Agent reactions require a forward migration".into(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::reaction;

    #[test]
    fn only_emoji_with_spaces_and_mentions_is_acknowledged() {
        for (content, reacted) in [
            ("👍", "👍"),
            ("  🎉\n", "🎉"),
            ("🎉🎉🎉", "🎉"),
            ("👍🏽", "👍"),
            ("❤", "❤️"),
            ("❤️ 🙏", "❤️"),
            ("[@Atlas](mention:agent:a1) 🙏", "🙏"),
            (
                "[@Atlas](mention:agent:a1) [@Nova](mention:agent:a2) 💯",
                "💯",
            ),
            ("🚚", "👍"),
            ("👩‍💻", "👍"),
            ("🇮🇳🇮🇳", "👍"),
            ("👍👍👍👍👍👍👍👍", "👍"),
        ] {
            assert_eq!(reaction(content), Some(reacted), "{content:?}");
        }
    }

    #[test]
    fn any_words_markup_or_many_emoji_still_ask_the_agent() {
        for content in [
            "",
            "   ",
            "[@Atlas](mention:agent:a1)",
            "ok",
            "👍 thanks",
            "Thanks 🎉",
            "👍?",
            "?",
            "1",
            "<b>👍</b>",
            "[@Atlas](mention:agent:a1) check stock 👍",
            "👍👍👍👍👍👍👍👍👍",
            "👍\u{200B}",
        ] {
            assert_eq!(reaction(content), None, "{content:?}");
        }
    }
}

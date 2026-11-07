//! Message search (schema 92): what someone can read, found by its words.
//!
//! PostgreSQL full-text search with the language-neutral `simple` configuration
//! (no stemming, so Hindi, English and mixed text all match as written), over an
//! index of sent and completed messages. Each word matches as a prefix, so a
//! partly typed word already finds results. Newest first.
//!
//! Access is the same as reading history: workspace membership, then public
//! conversations or ones the person is in. Agent replies then pass through the
//! same per-viewer view as history (`reply_views::hydrate`); any reply it hides
//! is dropped, and snippets are cut from the text the person may see.
use super::*;
use sea_orm_migration::prelude::{DbErr, MigrationName, MigrationTrait, SchemaManager};
use serde_json::json;

/// Results per page; one more is read to know whether there are older ones.
const PAGE: i64 = 30;
const MAX_TERMS: usize = 8;

/// Schema 92: the full-text index behind message search.
pub(crate) struct MessageSearch;
impl MigrationName for MessageSearch {
    fn name(&self) -> &str {
        "m20261107_000092_message_search"
    }
}
#[async_trait::async_trait]
impl MigrationTrait for MessageSearch {
    async fn up(&self, m: &SchemaManager) -> std::result::Result<(), DbErr> {
        // Builds once over existing messages; writes to the table wait meanwhile.
        m.get_connection()
            .execute_unprepared(
                "CREATE INDEX relay_chat_messages_search ON relay_chat_messages
                  USING gin (to_tsvector('simple', content)) WHERE state IN ('sent','complete');",
            )
            .await?;
        Ok(())
    }
    async fn down(&self, _: &SchemaManager) -> std::result::Result<(), DbErr> {
        Err(DbErr::Custom(
            "Message search requires a forward migration".into(),
        ))
    }
}

#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SearchInput {
    pub q: String,
    /// One conversation only.
    #[serde(default)]
    pub chat: Option<String>,
    /// One author only: a person's or an agent's id.
    #[serde(default)]
    pub from: Option<String>,
    /// Only messages with files.
    #[serde(default)]
    pub files: bool,
    /// Older than this message sequence: the next page.
    #[serde(default)]
    pub before: Option<i64>,
}

/// The words of a query, lowercased, without characters that mean something
/// to `to_tsquery`. Letters of every script and their marks are kept.
pub(crate) fn terms(query: &str) -> Vec<String> {
    query
        .split(|c: char| {
            c.is_whitespace() || "&|!()<>:*'\"\\,.;?/[]{}@#".contains(c) || c.is_control()
        })
        .filter(|word| word.chars().any(char::is_alphanumeric))
        .map(str::to_lowercase)
        .take(MAX_TERMS)
        .collect()
}

/// Every word must appear, each as a prefix: `'deliv':* & 'priya':*`.
pub(crate) fn ts_query(terms: &[String]) -> String {
    terms
        .iter()
        .map(|t| format!("'{t}':*"))
        .collect::<Vec<_>>()
        .join(" & ")
}

/// Readable text: mentions as @names, no Markdown marks, one line.
pub(crate) fn plain(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let mut rest = content;
    while let Some(start) = rest.find("[@") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match (after.find("](mention:"), after.find(')')) {
            (Some(name_end), Some(close)) if name_end < close => {
                out.push_str(&after[..name_end]);
                rest = &after[close + 1..];
            }
            _ => {
                out.push('[');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out.chars()
        .filter(|c| !"*_`#>".contains(*c))
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// About `width` characters around the first word that matches a term.
pub(crate) fn snippet(text: &str, terms: &[String], width: usize) -> String {
    let lower = text.to_lowercase();
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= width {
        return text.to_owned();
    }
    // Character index of the first word starting with any term.
    let hit = terms
        .iter()
        .filter_map(|term| {
            lower.match_indices(term.as_str()).find_map(|(byte, _)| {
                let starts_word = lower[..byte]
                    .chars()
                    .next_back()
                    .is_none_or(|c| !c.is_alphanumeric());
                starts_word.then(|| lower[..byte].chars().count())
            })
        })
        .min()
        .unwrap_or(0);
    let start = hit
        .saturating_sub(width / 3)
        .min(chars.len().saturating_sub(width));
    let end = (start + width).min(chars.len());
    let mut part: String = chars[start..end].iter().collect();
    if start > 0 {
        part = format!("…{}", part.trim_start());
    }
    if end < chars.len() {
        part = format!("{}…", part.trim_end());
    }
    part
}

impl PgStore {
    pub(crate) async fn search_messages(
        &self,
        user: &User,
        w: &str,
        input: &SearchInput,
    ) -> Result<Value> {
        ensure!(
            input.q.chars().count() <= 200,
            "Search for up to 200 characters"
        );
        let terms = terms(&input.q);
        ensure!(!terms.is_empty(), "Type a word to search for");
        let tx = self.read_tx(w).await?;
        // The index covers every workspace, so a word found in most messages
        // costs more as the platform grows; bound it like other heavy reads.
        execute(&tx, "SET LOCAL statement_timeout = '5s'", vec![]).await?;
        role(&tx, w, &user.id).await?;
        if let Some(chat) = &input.chat {
            super::chat::access(&tx, user, w, chat).await?;
        }
        let rows = rows(&tx, "SELECT m.id,m.chat,m.author,m.kind,m.content,m.state,m.at,m.thread,m.sequence,m.revision,m.progress,
              c.kind AS chat_kind,c.name AS chat_name,m.attachments<>'[]'::jsonb AS has_files,
              CASE WHEN m.kind='agent' THEN (SELECT a.name FROM relay_shared_agents a WHERE a.workspace=m.workspace AND a.id=m.author)
                ELSE (SELECT COALESCE(u.display_name,u.email) FROM relay_users u WHERE u.id=m.author) END AS name
            FROM relay_chat_messages m JOIN relay_chats c ON c.workspace=m.workspace AND c.id=m.chat
            WHERE m.workspace=$1 AND m.state IN ('sent','complete')
              AND to_tsvector('simple', m.content) @@ to_tsquery('simple', $2)
              AND (c.visibility='public' OR EXISTS(SELECT 1 FROM relay_chat_people p WHERE p.workspace=c.workspace AND p.chat=c.id AND p.user_id=$3))
              AND ($4::text IS NULL OR m.chat=$4) AND ($5::text IS NULL OR m.author=$5)
              AND (NOT $6 OR m.attachments<>'[]'::jsonb) AND ($7::bigint IS NULL OR m.sequence<$7)
            ORDER BY m.sequence DESC LIMIT $8",
            vec![w.into(), ts_query(&terms).into(), user.id.clone().into(), input.chat.clone().into(), input.from.clone().into(),
                input.files.into(), input.before.into(), (PAGE + 1).into()]).await?;
        let more = rows.len() as i64 > PAGE;
        let mut messages = Vec::new();
        for row in rows.into_iter().take(PAGE as usize) {
            messages.push(json!({
                "id": row.try_get::<String>("", "id")?, "chat": row.try_get::<String>("", "chat")?,
                "author": row.try_get::<String>("", "author")?, "kind": row.try_get::<String>("", "kind")?,
                "content": row.try_get::<String>("", "content")?, "state": row.try_get::<String>("", "state")?,
                "at": row.try_get::<i64>("", "at")?, "thread": row.try_get::<Option<String>>("", "thread")?,
                "sequence": row.try_get::<i64>("", "sequence")?, "revision": row.try_get::<i64>("", "revision")?,
                "progress": row.try_get::<Option<Value>>("", "progress")?,
                "chat_kind": row.try_get::<String>("", "chat_kind")?, "chat_name": row.try_get::<String>("", "chat_name")?,
                "has_files": row.try_get::<bool>("", "has_files")?, "name": row.try_get::<Option<String>>("", "name")?,
            }));
        }
        let oldest = messages.last().and_then(|m| m["sequence"].as_i64());
        super::reply_views::hydrate(&tx, user, w, &mut messages).await?;
        tx.commit().await?;
        let results: Vec<Value> = messages
            .into_iter()
            .filter(|m| m["response_unavailable"] != true && m["state"] != "cancelled")
            .map(|m| {
                let text = plain(m["content"].as_str().unwrap_or_default());
                json!({"id": m["id"], "chat": m["chat"], "chat_kind": m["chat_kind"], "chat_name": m["chat_name"],
                    "author": m["author"], "kind": m["kind"], "name": m["name"], "at": m["at"], "thread": m["thread"],
                    "has_files": m["has_files"], "snippet": snippet(&text, &terms, 160)})
            })
            .collect();
        Ok(
            json!({"results": results, "terms": terms, "next_before": if more { oldest } else { None }}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries_keep_words_of_every_script_and_drop_query_syntax() {
        assert_eq!(terms("Delivery  DATE?"), ["delivery", "date"]);
        assert_eq!(terms("it's a & b | !(c):*"), ["it", "s", "a", "b", "c"]);
        assert_eq!(terms("नमस्ते दोस्त"), ["नमस्ते", "दोस्त"]);
        assert_eq!(terms("@priya #sales"), ["priya", "sales"]);
        assert!(terms("&& ** ''").is_empty());
        assert_eq!(terms("a b c d e f g h i j").len(), MAX_TERMS);
        assert_eq!(ts_query(&terms("deliv priya")), "'deliv':* & 'priya':*");
    }

    #[test]
    fn snippets_read_plainly_around_the_first_match() {
        assert_eq!(
            plain("Ask [@Priya Menon](mention:user:u1) about **pricing**"),
            "Ask @Priya Menon about pricing"
        );
        assert_eq!(plain("a [link] stays"), "a [link] stays");
        let long = format!(
            "{} the delivery date moved to Friday {}",
            "intro ".repeat(40),
            "outro ".repeat(40)
        );
        let cut = snippet(&long, &["deliv".into()], 60);
        assert!(cut.starts_with('…') && cut.ends_with('…'), "{cut}");
        assert!(cut.contains("delivery date"), "{cut}");
        assert_eq!(snippet("short text", &["short".into()], 60), "short text");
        // Matches a word start, not the middle of another word.
        let s = snippet(
            &format!("redelivery {} delivery", "x ".repeat(60)),
            &["deliv".into()],
            30,
        );
        assert!(s.contains(" delivery"), "{s}");
    }
}

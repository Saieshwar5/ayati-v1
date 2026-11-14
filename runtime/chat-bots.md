# Bots in channels (schema 119, 2026-11-14)

Monitoring, CI, a shop or a billing system can post into a channel without a new
integration: each bot has a Slack-compatible incoming webhook address. Built from the
Ember review (item 7 of the chat improvements).

## Adding and removing

- **Who:** the channel's creator or a workspace admin, in Details → Bots ("Creator and
  admins"), the same people who change its settings. Direct messages have no bots.
- **Add a bot:** a name (1 to 60 characters). The answer has the address,
  `{origin}/hooks/chat/{bot id}/{secret}`, shown once with Copy: only the secret's
  SHA-256 is kept. At most ten bots per channel.
- **Remove:** the address stops working at once; the bot's messages stay, with its
  name. Creating and removing are audit-logged (`chat_bot_created`,
  `chat_bot_removed`).
- API: `GET` and `POST /api/identity/workspaces/{w}/chats/{chat}/bots` (`{name}` →
  `{bot, url}`), `POST …/bots/{bot}/remove`. Bots are listed as `{id, name, created,
  last_post, posts_30_days}`, never with their secret.

## Posting

`POST /hooks/chat/{bot}/{secret}` with Slack's JSON. No session, Origin or
`X-Ayati-Request` is needed (the secret is the credential; `chat::bot_hook_path`
exempts exactly this shape from the browser checks), and the body is at most 64 KiB.
Answers are Slack's words: `ok`, `invalid_payload`, `no_text`, `no_service` (unknown
bot, wrong secret or removed: the same answer for each), `channel_is_archived` (410),
`rate_limited` (429, more than 30 posts in a minute) and `try_again` (503).

The payload becomes Cuckoo Markdown (`chat/bots.rs`):

- `text`, unless there are `blocks`, which Slack shows instead.
- `<url|label>` and `<url>` become links (http, https and mailto only); `<@U123>`,
  `<#C1|name>` and `<!here>` become plain words.
- Blocks: `header` bold, `section` text, fields one per line and a link accessory,
  `context` in italics joined by " · ", `divider` a rule, `actions` buttons that have a
  `url` as links. Buttons without a `url` need Slack's interactivity and are left out;
  `image` blocks are left out (pictures are never loaded from other sites).
- Legacy `attachments`: the title (a link with `title_link`), the text or fallback,
  and fields as "**Title**: value".
- At most 12,000 characters. Form-encoded (`payload=`) bodies are not accepted.

## What a bot can't do

- A bot is its own author kind (`relay_chat_messages.kind = 'bot'`), never a person
  or an agent: no membership, no receipts of its own, no profile.
- Its message is an ordinary sent message: it asks no agent and starts, approves or
  cancels nothing. People reply in its thread, quote it, or @mention an agent about it,
  which asks the agent as usual with that person as the requester.
- Link targets that mean something inside Cuckoo lose their target before saving:
  `[@Ravi](mention:user:…)`, run links and workspace paths render as plain text. So a
  bot cannot mention someone, set off their alerts or open things in Cuckoo.
- Agents read bot messages in context as "Uptime Monitor (bot)", untrusted data like
  any message. Search finds them with the bot's name. They count as unread for people
  and get no link preview card.
- Bot names are added after loading (`chat_bots::name_bots`), so the shared message
  queries keep working on schemas before 119 (upgrade tests).

## Checks

Rust `chat::bots` tests (Slack text, blocks and attachments into Markdown; mentions,
runs, workspace paths and script links neutralised; the 12,000-character cut; the
hook address shape). PostgreSQL `chat_bots_tests` (managers only, the secret shown
once and never listed, wrong or unknown secrets, no job, kind and name in history,
unread and search, removal, 30 a minute, archived channels, ten per channel) and the
schema upgrade tests. Web `tests/chat-bots.test.mjs` (add, the address once with
Copy, remove with confirmation; a bot message's BOT tag, no profile and no preview).

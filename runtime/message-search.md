# Message search (schema 92)

People find messages by their words, in every conversation they can read or inside
one conversation. Choosing a result opens that conversation at the message; a thread
reply opens its thread with the reply marked.

## Where

- **Chat → Search** (the magnifier above the conversation list) searches
  conversations, people and agents by name, and messages by words. **Messages** shows
  only messages.
- **Search in this conversation** (the magnifier in a conversation's header) searches
  that conversation only.

Searching starts after two letters and a 250 ms pause in typing. A slower answer for an
older query never replaces the current one; leaving or changing the query cancels the
request. Results are newest first, 30 at a time, with **Older messages** for more. Each
shows who wrote it, where (in Search), whether it is in a thread or has files, when,
and a short excerpt with the matching words marked.

### Filters and recent searches (2026-11-14)

- **Chips** under the search box toggle: From me, Mentions me, From an agent, Has a
  link, Has a file, In threads. They combine.
- **Written filters.** `in:#orders` (a conversation, by name or the start of it),
  `from:Meera` (a person or agent, by full name or any word of it, or `from:me`),
  `has:file` and `has:link`. They are taken out of the words and shown as chips with
  × to remove them from the query. A written filter that names nothing visible says
  "Nothing matches in:#nowhere; searching without it." Parsing is in the browser
  (`web/src/react/chat/search-query.ts`), so names resolve against the directory the
  person can see; the server only receives ids.
- **Recent searches.** Opening a result remembers the query (five at most, per person
  and workspace, in local storage). With the search box empty, Search shows them as
  chips to run again, and Clear.

## How it matches

PostgreSQL full-text search with the language-neutral `simple` configuration: no
stemming and no stop words, so Hindi, English and mixed text match as written. Every
word must appear, and each word matches as a prefix, so `deliv` finds "delivery" and
"delivered". Case and punctuation don't matter; query syntax characters are dropped;
at most 8 words and 200 characters.

Only sent and completed messages are indexed. An edit changes what is found; a deleted
or cancelled message is not found. Excerpts show mentions as `@name` and drop Markdown
marks.

## Who sees what

The same rules as reading history, checked on every request:

- the person must be a member of the workspace;
- public conversations, and private ones they are in;
- searching one conversation needs access to it;
- agent replies go through the same per-viewer view as history
  (`reply_views::hydrate`), and any reply it hides is dropped, so excerpts come only
  from text the person may see.

## API

`GET /api/identity/workspaces/{workspace}/message-search?q=…`, with optional `chat`
(one conversation), `from` (one person or agent), `files=true` (only messages with
files), `mentions_me=true` (messages that @mention the person searching),
`agents=true` (agents' messages), `links=true` (an `http://` or `https://` link in the
text), `threads=true` (thread replies) and `before` (the `next_before` of the previous
page). A word is still required: filters narrow the indexed word search and never scan
a workspace on their own. The answer is
`{results, terms, next_before}`; `next_before` is `null` on the last page.

## Cost and limits

Schema 92 adds one partial GIN index,
`relay_chat_messages_search` on `to_tsvector('simple', content)` for sent and
completed messages. It is built once when the migration runs, and writes to the
messages table wait while it builds; on a large existing table, run the migration in
a quiet period.

Measured locally (debug build, 200,000 messages over ten workspaces, 20,000 in the one
searched): PostgreSQL combines the search index with the workspace index. Searches
took 2–8 ms; a word in every message took 27 ms, and a one-letter word 25 ms. The index
spans every workspace, so a word found in most messages costs more as the platform
grows; each search has a 5-second statement limit, like other heavy reads. A composite
workspace-and-words index (`btree_gin`) or per-workspace partitioning is the next step
if hosted measurements show it is needed.

Not included: ranking by relevance (results are newest first), stemming or synonyms,
searching inside attached files, and saved searches shared across devices.

## Checks

- Rust unit (`src/identity/postgres/message_search.rs`): query words in every script,
  query syntax dropped, the 8-word cap, plain excerpts around the first matching word.
- PostgreSQL (`src/identity/postgres/message_search_tests.rs`): prefixes, Hindi, every
  word required, newest first, private conversations, another workspace, one
  conversation, one author; edits and deletions; paging 35 results as 30 + 5; the
  mentions, agent, link and thread filters, alone and combined.
- Web (`web/tests/message-search.test.mjs`): one request after typing pauses, results
  and marked words, the Messages filter, a slow older answer never replacing the
  current one, searching one conversation and opening its message, older results and
  a failed search; chips and written filters in the request, removing a written
  filter, an unknown one, recent searches and Clear; name resolution.
- Live browser (simulation server): a member doesn't find a private conversation they
  aren't in; a thread reply opens its thread; a message 40 messages back opens in
  view; light and dark; no sideways scroll at 390 px.

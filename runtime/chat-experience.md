# Unified chat composition and readable responses

Channels, shared direct messages, threads and owner-private agent conversations now
use the same React composer. The private transcript retains its existing tools,
approvals, artifacts and event transport; its old DOM composer has been removed.
A React-owned island is explicitly mounted/disposed within the compatibility subtree.
Private drafts and submissions retain their existing per-agent/account/workspace
storage; shared chat retains its durable IndexedDB outbox and exact retry IDs.

## Response rendering

Both transcript renderers and pending shared messages use `web/src/markdown.ts`.
Marked parses GFM headings, lists, emphasis, quotations, tables and fenced code;
DOMPurify applies an explicit tag/attribute allowlist before HTML reaches the DOM.
Raw HTML is escaped. Model-provided images are text, not automatic remote requests;
authenticated uploaded attachments keep their existing media renderer. Links allow
HTTP(S) and mailto, with noopener/noreferrer. Code and tables scroll locally.

Dependency choice: Marked 18.1.1 (MIT) and DOMPurify 3.4.17 (Apache-2.0 OR MPL-2.0) provide one focused parser/sanitizer boundary usable in
React and the remaining DOM transcript. No editor framework is added. The real
integration tests cover formatted text, unsafe links/HTML and partial streamed input.
The combined entry bundle grows from 93.73 kB gzip to about 122 kB gzip in the local
production build; the lazy private-chat bundle remains about 18 kB gzip. This cost
includes the composer and media/mention changes, not only the two libraries.

## Code, long messages and link previews (schema 118, 2026-11-14)

- **Copy code.** Every fenced code block (`markdown.ts`, so people's messages, agents'
  replies and the reader alike) gets a Copy button that copies just the code and says
  "Copied". One delegated click listener per window handles it, so HTML set from
  Markdown needs no handlers. Typed HTML is still escaped: only the renderer makes the
  button, which the sanitizer allows with `type` and `aria-label`.
- **Show more.** A person's message over 1,200 characters or 14 lines folds to about
  16 lines with a fade, and "Show more" / "Show less" (`aria-expanded`). Agents'
  replies keep their own expanding reader.
- **Link previews.** The first http(s) link of a person's sent message gets a card:
  site, title and up to two lines of description, linking to the page
  (`noopener noreferrer`). Never a picture: loading a remote image would tell that site
  who reads the conversation. The browser asks
  `GET …/chats/{chat}/messages/{message}/link-preview` once per message and revision,
  and only when the message scrolls into view.
  - The server reads the link from the message itself, after the same access check as
    reading the conversation, so nobody can make it fetch an arbitrary address.
    Agents' replies get no preview (they have per-viewer views of their own).
  - Fetching is fenced like a remote MCP connection (`chat/link_preview.rs`): http(s)
    on ports 80 and 443, no credentials in the address, the host resolved once and
    pinned to its public IPv4 addresses (private, loopback, link-local, CGNAT and the
    cloud metadata address are refused; IPv6-only hosts get no preview), each of up to
    three redirects checked again, no proxy, 5 seconds, `text/html` only, 256 KiB
    read. At most four pages are fetched at once per process; a busy server answers no
    preview rather than queueing.
  - The page's own Open Graph, Twitter and plain `description`/`<title>` tags give the
    preview, with entities decoded and lengths cut (title 200, site 80, description
    300). The site falls back to the host without `www.`.
  - `relay_link_previews(url, preview, fetched)` (schema 118) caches an answer a day
    and a page without a preview an hour. It holds what public pages say about
    themselves, never who posted or read a link.
- **Checks.** Rust `link_preview_tests` (first link, meta tags in order, entities,
  multibyte text; a local server for redirects, non-HTML and missing pages; refusing
  this machine, `localhost`, private and metadata addresses, credentials and `file:`);
  PostgreSQL `link_previews_tests` (only readable people's messages, deleted ones none,
  the day and hour cache); web `tests/chat-message-text.test.mjs` (Copy copies the
  code, typed buttons are text, folding, one read per message on screen, no read
  without a link or for an agent, only http(s) cards, no picture).

## Bots

Channels can have bots that post through a Slack-compatible webhook, tagged BOT and
never able to ask agents or approve anything: see [bots in channels](chat-bots.md).

## Mentions

Typing `@` searches eligible people and requestable agents. Arrow keys move through
suggestions; Enter/Tab selects; Escape dismisses. Enter sends, Shift+Enter inserts a
line break, and input-method composition does not submit. Selected names remain
readable in the input while canonical content stores `[@Name](mention:user:id)` or
`[@Name](mention:agent:id)`. Editing inside a selected name removes its identity;
editing around it preserves it. Draft reload and outgoing retries preserve the exact
serialized content. Names may coincide without changing the referenced identity.

Private channels suggest their people; public channels may mention workspace members
who already have access. The server validates membership on a new send within the
write transaction. Every mentioned agent must be one the message asks, and each passes
the existing requester and channel checks; a message that mentions an agent without
asking it is refused with a message saying so. Multiple human mentions are supported.
Ordinary `@text` does not dispatch work. An edit checks the
mentions it adds with the same rules (people who can read the conversation, well-formed
identities). An edit never asks an agent, so adding an agent mention is refused with
"Editing does not ask an agent. Send a new message to ask it."; mentions already in
the message stay as they are. Human mentions are
highlighted in the conversation (a whole message that mentions you also stands out, see
below) and participate in ordinary unread/receipt behavior;
this increment does not add a separate mention inbox, email or push notifications.

## Mentions of you, quick edit and shortcuts (2026-11-14)

- **Mentions of you stand out.** A person's message that @mentions you gets the warm
  warning surface and "mentions you" after the time, so it does not rely on colour
  alone (forced colours outline it). Your own messages and agents' replies are not
  highlighted; agents' replies to you already turn the unread count red.
- **↑ edits your last message.** In an empty composer, ↑ (no modifier) opens your
  newest sent message in the conversation for editing, the same editor as Edit
  message; Enter saves, Escape cancels and focus returns to the composer. With a draft,
  ↑ moves in the text as usual. Editing is held by the conversation
  (`ConversationPane` → `ChatMessage editing`), so the composer can start it.
- **Shortcuts** (`web/src/react/chat/chat-shortcuts.tsx`, one window listener while
  Chat is open): ⌘K / Ctrl+K opens Search; ⌘/ / Ctrl+/ lists every shortcut; Alt+↓ and
  Alt+↑ open the next or previous conversation in the list as it reads (pinned,
  channels, direct messages, after the Unread only and hidden filters); Alt+⇧+↓ and
  Alt+⇧+↑ the next or previous unread one. Arrows are left alone while a dialog is
  open. The list shows ⌘ and ⌥ on Apple devices and Ctrl and Alt elsewhere, and names
  only shortcuts that exist.
- **Checks.** `web/tests/chat-shortcuts.test.mjs`: the highlight on a mention of you
  only, ↑ with and without a draft, the newest own message, Escape returning focus,
  saving the edit, ⌘K, ⌘/, Alt+↓ and Alt+⇧+↓.

## Several agents in one message (schema 99)

The owner's decision of 2026-11-07 reverses the one-agent rule of 2026-10-21: a message
may @mention and ask **up to 3 agents**, and each answers separately, in its own reply,
with its own job, permissions, budget, model and receipts. The picker offers more agents
("Also asks this agent", "Already asked") until three are asked, then shows the others as
"Up to 3 agents per message"; sending a fourth says "Ask at most 3 agents in one message.
Send another message for the others." and keeps the draft. While a send is unconfirmed,
the pending message lists "Asking Atlas…" for each agent; once accepted, each agent's
reply waits, types and finishes on its own. A direct message with an agent asks only that
agent. Quoting an agent's message or replying in an agent's thread still asks that agent
when the text names none.

- **API:** `POST …/messages` takes `agents: [id, …]` (1 to 3, each once) and still
  accepts the older single `agent` (not both). Background requests take the same fields.
- **All or nothing:** every agent passes every check (may this person ask it here, paused,
  model allowed, monthly budget, in the conversation) and the queue has room for all of
  them, or nothing is saved. One refusal names each agent and its reason, for example
  "Vega can't take this message: Agent is not shared with you. Nothing was sent; remove
  @Vega to ask the others." A partial send would leave a message that mentions an agent who
  was never asked, which reads as if it was, and an outbox retry would have to know which
  agents were left out. With all or nothing, the message and one job per agent exist
  together or not at all, so the outbox's exact replay creates nothing new. Asking one
  agent keeps the reasons people saw before.
- **After acceptance** each job is independent: one agent failing, being stopped,
  answered at the front desk or refused by the gateway (402 `budget_reached`) never blocks
  the others. Deleting the message stops every unfinished job; Retry in Activity asks only
  that agent again (other agents' mentions become plain names); seen ticks are per agent.
- **Instructions:** each agent is told, by name, which other agents the same message
  asked, to answer only its own part and not to answer, speak or hand off for them. Each
  answers from the conversation as it was when asked: another agent's reply to the same
  message is left out of its context, whichever finishes first.
- **Storage:** schema 99 (`m20261107_000099_several_agents_per_message`) replaces
  `relay_chat_jobs`' unique request message with `relay_jobs_request_agent`, unique per
  workspace, request message and agent. The message's `target` keeps the first agent asked.
- **Checks:** PostgreSQL `chat_intake::several_agents_tests` (two replies and jobs, exact
  replay, refusals by name for access, pause, budget, four agents and the queue, delete,
  retry, receipts, instructions and context, the 98 → 99 upgrade); web
  `tests/chat-several-agents.test.mjs` and `tests/chat-composer.test.mjs`; journey
  `tests/several_agents.mjs` (real pinned Goose, scripted provider: one message, two
  pooled runs, two replies, each run told about the other agent).

## Agents talking in the conversation (schema 100, D246)

When an agent hands work to another agent, the people there see them talk, with the
approved [handoff card](../designs/channel-agent-handoffs/README.md). Where the
request was asked (the main timeline, or its thread) they read, in order:

1. the person's request ("@Atlas ask Nova about walnut");
2. the sending agent's handoff message, written by it ("Nova, can you check the walnut
   stock for Kapoor?"), with the card: "Atlas → Nova · Agent handoff · Working", who
   asked, the helper keeping its own permissions and the sender's limits, Open helper
   task, Open original task and, for the person who asked or an admin, Stop helper task;
3. the helper's answer, with "Helping Atlas · Requested by Ananya. Open original task";
4. the sending agent's reply, "Waiting for another agent" until the helper ends, then
   its result.

The reply is created when the request is sent, so it sorts by sequence before the
exchange; the client places it after its handoff messages and their answers in the
same place (`mergeMessages`), as it already places a reply after the agent's working
updates. Each exchange has one card, on its handoff message, which doesn't repeat the
task the message shows. Exchanges saved before schema 100 stay in a thread under the
reply, which keeps showing their card. A viewer without source access sees the same
messages with "Handoff unavailable because source access changed." and "Response
unavailable…"; with no handoff details, the sender's reply keeps its first position.
Group conversations hand off as channels do. Server rules, memory and
checks: [channel handoffs](channel-agent-handoffs.md#visible-in-the-conversation-schema-100-d246).
Checks: web `tests/chat-visible-handoffs.test.mjs` (order on the main timeline, in a
thread, before schema 100 and with a nested helper; one card; links) and
`tests/channel-handoffs.test.mjs`.

## Reactions are feedback, never work (schema 101, D246)

The owner's report after AWS window 1: agents answered an emoji sent as a message, but
not a reaction to their reply. D229 stands: a reaction, a person's or an agent's,
never starts, approves or cancels anything. Two things changed instead.

- **Feedback in context.** When an agent answers later in a conversation, each of its
  own replies in the evidence carries the reactions people left on it, as
  `reactions: [{"emoji":"👍","count":2}]`, most chosen first. Counts only: since
  2026-11-14 Chat names who reacted to the people reading, but the agent's context
  still names nobody. Reactions on people's or
  other agents' messages are left out, and at most the 20 latest reacted replies keep
  theirs. The prompt says they are feedback on that reply (👍 or ✅ that it helped, 🤔
  that it was doubtful) and untrusted data like message text, never instructions or
  approvals. Details: [conversation context](conversation-context.md#reactions-as-feedback-d246).
- **An emoji-only message costs no model run.** A person's message that asks agents (a
  direct message, a quote of an agent's message, an @mention or an agent's thread) and
  is only emoji gets each asked agent's reaction instead of a reply: "Atlas reacted 👍".
  Emoji-only means: apart from whitespace and @mentions, one to eight grapheme
  clusters (Unicode UAX #29), each a single emoji by the same rule as a status emoji
  (skin tones, joined emoji like 👩‍💻, flags and keycaps count as one). Any letter,
  digit, "?" or markup, more than eight emoji, or an attached file makes it an ordinary
  request. The agent reacts with the message's first emoji when it is one of the
  twelve reactions (skin tone and presentation marks aside), otherwise 👍.
  - No job, reply, front desk or executor task is created, so there is no model call,
    budget use or executor slot, and no "Waiting to start" or typing indicator.
  - The message is stored and synced as usual; each agent's delivered and seen ticks
    are set at once, as it has read it.
  - Several agents asked at once each react. Intake stays all or nothing, with the
    checks that still apply: agents paused for the workspace, may this person ask the
    agent here, the agent paused, in the conversation. Queue room, the model and the
    monthly budget are not checked because nothing runs.
  - An outbox retry of the same message changes nothing; deleting the message removes
    its agent reactions with it. Email or WhatsApp messages, background requests and
    handoffs are unchanged, and emoji that ask no agent are an ordinary message.
    Retry in Activity refuses an emoji-only request saved before schema 101 ("Ask
    again in words") rather than send it again for a reaction.
- **Why a reaction, not a short reply.** A reaction is the acknowledgement people
  already use for "thanks 👍": it adds no message to sort, notify, count as unread or
  feed into later context, and it costs nothing. A canned text reply would do all of
  those while saying less.
- **Client.** Agent reactions show under the message beside people's chips, each with
  the agent's face and name ("Atlas reacted 👍"). They are not toggles; people's chips,
  quick reactions and the picker are unchanged.
- **API and storage.** Message payloads (history, live changes, the accepted message)
  add `agent_reactions: [{"agent", "emoji", "name"}]`; `reactions` keeps only people's.
  Schema 101 (`m20261107_000101_agent_reactions`) adds `relay_agent_reactions(workspace,
  chat, message, agent, emoji, at)`, one per agent and message, removed with its
  message; `relay_message_reactions` is unchanged. A change refreshes only that message
  on open screens. The grapheme split uses `unicode-segmentation` 1.13.3 (MIT or
  Apache-2.0), already in the dependency tree.
- **Checks.** PostgreSQL `reaction_feedback_tests` (feedback with counts at intake and
  execution, people's messages left out, a reaction creates no job, the 20-reply bound)
  and `agent_reaction_tests` (an emoji-only direct message: no job, reply or executor
  task, the agent's reaction and ticks, retry and
  delete; two agents and a quote; an agent past its budget still reacts and a paused
  one refuses; emoji with words still runs; the 100 → 101 upgrade);
  unit tests for emoji-only; web `tests/chat-message-actions.test.mjs`
  (name, face, not a toggle, picker unchanged) and `tests/chat-several-agents.test.mjs`
  (an emoji-only send shows both agents' reactions and no waiting or typing reply).

## Word reactions and who reacted (2026-11-14)

- **Words.** "On it", "Done" and "Seen" are reactions beside the twelve emoji
  (`WORD_REACTIONS` in `postgres/message_actions.rs` and `message-actions.tsx`), offered
  across the top of the picker. They are a quick status without a message, and like
  emoji they never start, approve or cancel anything. The stored value is the word
  itself (within the existing 8-character limit); no migration was needed. An agent's
  own reaction to an emoji-only message still uses the emoji set only.
- **Who reacted.** Each of people's reactions now carries `users`: the first ten
  people who chose it, earliest first (history pages, focused pages and live changes).
  The chip's title and accessible name say "Ravi Kumar, You, A former member and 2
  others reacted 👍", then "Remove yours" or "Add yours". The names come from the
  directory the reader already sees; a server without `users` reads "3 people reacted".
- **Checks.** PostgreSQL `message_action_tests` (word reactions accepted, other words
  refused, `users` in order on live and history payloads) and the schema 101 upgrade
  test; web `tests/chat-message-actions.test.mjs` (names on the chip, a word chip, the
  three words in the picker and their requests, wording for one, two and many).

## Presence and status (D245)

People show who is around (`designs/chat-and-account` screens 2, 3, 9, 10): a green
dot on the face of anyone who had Cuckoo open in about the last minute, no dot when
away, and "Last seen 9:58 AM" (or a date) in a direct message's header. Agents never
show presence. A status is one of Available, Working, Busy, In a meeting, Away or your
own text (80 characters) with an emoji, and clears after 1 hour, today, this week or
never; it shows after the name in the Chat list, in the profile, in the @mention list
and under your name in the account menu, which opens Set a status. Away shows no dot
even with Cuckoo open. Busy quietens desktop alerts to direct messages, @mentions and
replies to your requests. "Show when I'm online" (Settings → Preferences) is one-sided:
off hides your dot and last seen from everyone, while you still see theirs.

The header no longer shows your own connection; a thin banner appears only while you
are offline or reconnecting. The browser keeps one presence store fed by
`/chat-directory` and the live stream's `presence` events, with no extra polling.
Server design, write rate and privacy: [presence and status](reliable-messaging.md#presence-and-status-schema-98-d245).

## Threads beside the conversation (schema 115, 2026-11-14)

A thread opens in the panel beside the conversation that Details and profiles use
(`web/src/react/chat/thread.tsx`, `SidePanel`), so the conversation stays readable
while replying; it is no longer a modal drawer. The thread and Details take turns:
opening one closes the other. Escape or × closes the thread and focus returns to
what opened it. The replies scroll and the composer stays at the bottom of the panel.

- **Summary under the first message.** "4 replies · last 10:52" (a date on another
  day). The server returns `thread_replies` and `thread_last` with every message
  (history pages, focused pages and live changes): replies that are not working
  updates and not deleted, counted through `relay_messages_by_thread`. Older clients
  and pages before schema 115 fall back to counting the loaded replies.
- **Live.** Adding or deleting a reply records a change for its root
  (`relay_thread_reply_changed`), so every open screen refreshes the count without
  reloading the thread.
- **Also send to the conversation.** Under the thread composer, "Also send to
  #orders" ("the conversation" in a direct message) posts the reply in the thread and
  shows it in the main timeline too, with "Replied in a thread: …" and View thread.
  The choice is for one reply. `POST …/messages` takes `broadcast: true` only with a
  `thread` ("Only a thread reply can also be sent to the conversation"); the column
  `relay_chat_messages.broadcast` has a matching check. An outbox retry must repeat
  the same choice ("Message ID already used" otherwise), and ordinary replies keep
  sending the earlier payload. It still counts as a thread reply, and agents asked in
  it answer in the thread.
- **Checks.** PostgreSQL `chat_threads_tests` (count and last time, the root's live
  change on a new and a deleted reply, broadcast only from a thread, the saved flag,
  exact retries); web `tests/chat-threads.test.mjs` (server count over the loaded
  page, the main timeline shows only broadcast replies with their thread line, the
  side panel, the checkbox payload and reset, Details taking turns, reply times).

## Replies and media

An agent requested in the main channel responds in the main channel timeline.
Only an explicitly threaded request creates a threaded response. Quoting an agent's
message asks that agent, and in a thread whose first message an agent wrote, a reply
with no agent mention asks that agent; an explicit agent mention wins in both. The
server still checks that the person may ask that agent here. Existing stored
threads remain untouched. Request context remains narrowly scoped as before; changing
reply placement does not expose unrelated threads or private-agent memory. Shared
agent names accompany authorized messages so readers can identify agents even when
they lack permission to invoke them.

Attachment and voice buttons live inside the composer. Since D245, Send or Enter sends a
recording at once and the mic with × or Escape cancels it; failed sends keep it with Retry and
Delete (see [voice messages](unified-chat.md#voice-messages-d245)). Media tracks, the live level's
audio graph and animation frame, observers and pending requests are released on teardown. Shared
upload limits remain 10 MiB/eight files; private uploads retain their existing 25 MiB/eight-file
boundary. Speech recognition is described in [voice transcription](voice-transcription.md).

Messages retain own-right/other-left alignment, quieter incoming surfaces and compact
consecutive sender groups. Own messages omit repeated names/avatars, with time and
accessible delivery indicators on the final line. Thread actions remain keyboard/touch
accessible as compact reply buttons. The single-row composer floats inside the work
area with an inset and soft shadow, growing only for drafts, files or recording controls. Sent/delivered/seen states remain distinct. Screens are
checked at desktop and 360px widths with local scrolling.

## Verification

Run the normal Rust/web checks, then with the isolated company server running:

```sh
./tools/company-simulation.sh test
node tools/company-simulation/chat-ui.mjs
node tools/company-simulation/messaging.mjs
```

The UI rehearsal uses real shared APIs and deterministic agents, retains its messages,
files and screenshots, and makes no LLM calls. Its private-agent response/health
endpoints are mocked in the browser to verify formatting and draft retention without
provider costs. This is not a live model/tool execution test.

Verified locally: 58 Rust library tests, 21 PostgreSQL integration tests, strict
clippy, 16 frontend test files, production build, browser-test type checking and
362 source files within 500 lines. Seven real-browser UI scenarios and eight existing
messaging recovery scenarios pass. Recovery samples showed 129–163 ms display latency
for 12 messages; this remains a small local measurement, not a production guarantee.

Dependency audit reports no advisory for the new parser/sanitizer. It reports an
existing Vite 8.3.2 development-server launch-editor advisory; that unrelated tool
upgrade is not included in this change. Normal use here serves built static assets.

## Compact design follow-up

The approved floating-composer design passes all 16 frontend test files, production
build, browser-test type checking, seven browser UI scenarios and eight recovery
scenarios. Stop-to-attach, failed-upload retry and cancellation are covered explicitly.
Layout checks cover a single-row empty composer, desktop, narrow and 320px dark/short
windows. The offscreen receipt check uses a short viewport because compact messages
can now all fit in the previous desktop viewport. No real provider calls were made.

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
highlighted in the conversation and participate in ordinary unread/receipt behavior;
this increment does not add a separate mention inbox, email or push notifications.

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
  `reactions: [{"emoji":"👍","count":2}]`, most chosen first. Counts only: Chat shows
  nobody's name on a reaction, so neither does the context. Reactions on people's or
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

## Replies and media

An agent requested in the main timeline of a channel or group several people read
answers in a thread under the request, unless the channel's managers chose "In
the channel" ([threads in channels](channel-threads.md), schema 123); elsewhere it
answers in the main timeline. Quoting an agent's
message asks that agent, and in a thread a reply that names nobody asks the agent a person last asked
there (or the agent that started it); naming anyone wins. The
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

Messages retain own-right/other-left alignment and compact consecutive sender groups.
Since 2026-11-21 both sides use the same raised bubble (white with a soft shadow in light
mode) instead of a grey fill for your own messages and none for others, and a long
message from a person starts closed with a corner arrow to open it
([design system](design-system.md#chat)). Own messages omit repeated names/avatars, with time and
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

## First visit (2026-11-20)

Chat with nothing open is a start area, not a welcome sentence
(`web/src/react/chat/chat-start.tsx`). Who can be added comes from the chat
directory already loaded (`start-model.ts`): people other than you, team agents
(never an assistant, which joins no channel) and whether you are an admin. Admins
also read their pending invitations (`GET …/people`); members read nothing extra.
Everything shown is presentation: the server authorizes invitations and conversations.

| Situation | Start area | Channels ＋ | Direct messages ＋ |
|---|---|---|---|
| Admin, nobody else | Assistant · Invite people · Create agent | Add someone first | Nobody to message yet (Invite first) |
| Member, nobody else | Assistant · Create agent | Add someone first (Create agent) | Nobody to message yet (Create agent) |
| Team agents, no people | as above | Create channel form, + Invite people row (admin) | New message list, + Invite people row (admin) |
| People here | Assistant · faces + All · Invite (admin) · Create agent | Create channel form | New message list |

An invitation counts as waiting while it is `pending` and unexpired; one whose
email delivery is unconfirmed grants no access and is not shown. Create agent links
to `?view=agents&new=1`, which opens the create steps once (resuming an unfinished
agent) and removes the flag from the address. Chat's dialogs now start in their text
field: React's `autoFocus` ran while the `<dialog>` was closed, so `showModal()` put
focus on Close; they pass `initialFocus` instead. Agent names in the channel picker
are no longer cut short (`.chat-choice > span`).

Checked: `tests/chat-first-visit.test.mjs` (roles, the ＋ choice and focus return,
one-click direct messages, dialog focus, examples that never send, the Create agent
link) and `tests/chat_first_visit.mjs` against a real controller and Chromium.
Library, Agents and other dialogs that use `autoFocus` inside `Dialog` have the same
focus issue and are not changed here.

## Conversation header and agent state (2026-11-20)

The header is 56 px (was 64), with the list's Chat title. Its face, name and the
line under the name are one target with a ▾ that opens the profile or Details; the
name is the button ("orders, details", "Meera Iyer, profile"), and the separate
Profile and Details buttons are gone. A team agent's owners and admins get its
settings (Instructions) beside search, as your assistant always had.

The chat directory's agents carry `paused` (settings), `needs_owner` (no owner) and
`all_paused` (an admin paused all agents). Chat shows them as an agent's dot and
line, and in a direct message replaces the message box with a quiet bar while the
agent takes no messages; its owners and admins resume it there. "Working on your
request…" (or "Waiting for approval", "Waiting for an answer") comes from the
agent's reply still in progress in that conversation, so no server change. Pausing
or resuming one agent now announces an `agents` change, as pausing all agents
already did, and open Chat pages read the directory again.

| State | Dot | Line under the name |
|---|---|---|
| Ready | green | Ready · its job |
| Working for you here | purple, pulsing | Working on your request… |
| Paused by an owner | hollow | Paused · Ananya Rao can resume it (You can resume it) |
| Last owner left | hollow | Paused · Its owner left · an admin can give it a new one |
| All agents paused | hollow | Paused · An admin paused all agents |

Checked: `web/tests/chat-agent-state.test.mjs` (each state, the bar, a member
without Resume, one resume for a double click, the `agents` change, working and
waiting) and the PostgreSQL pause test (`activity_controls_tests.rs`: the directory
says paused and resumed, and the change is announced).

## Profiles and channel Details (2026-11-20)

Before, a profile had two versions: from a direct message's header it showed the
bell and Pin above the person and ended with "Direct messages have no member list,
settings or archive."; from a message or People & agents it was titled "Profile" with
Message and no bell. Channel Details held its whole settings form for managers
(about 1,290 px in a 360 px column) and four explanatory sentences.

Now one layout serves every profile (`profile-card.tsx`) and the panel's actions are
one row of round tiles (`panel-actions.tsx`) in profiles and Details alike:

| Action | Shown |
|---|---|
| Message | outside your direct message with them; never to yourself |
| Notifications, Pin | inside your direct message with them |
| Resume | a paused agent, to its owners and admins (first) |
| Add to channel | an agent, when you manage a channel it is not in |
| Settings | an agent, to its owners and admins (its Instructions page) |
| Edit profile | your own profile (Settings → Profile) |

Facts are icon rows: About or Handles, email with Copy, the agent's owners (each
opens their profile), and shared channels as links. Channel Details keeps its
facts, round actions (bell, Pin, Archive), members with one ＋ Add for people and
agents (`channel-add.tsx`; assistants and "Only me" agents are not offered), a
search box only past eight members, a "Channel settings ›" row to a step of its own
(`PanelView.step`), and Leave. Settings use switches (real radio buttons); the only
words left warn about making the history public and about a non-admin choosing
"Only admins". No server change.

Checked: `web/tests/chat-profile.test.mjs` (Resume once and Settings for an owner,
none for a member, the owner row, Edit profile and Copy on your own profile, search
past eight members) and the updated Details and People tests.

## Pictures, files and voice in a message (2026-11-21)

Before, every attachment went through one row: a bordered button with the file's
name, a loose "· 368 KB" and a bare arrow. A file read like a form field inside its
bubble, and a picture had that row repeated under a 192 px thumbnail.

Now each kind has its own shape, all at one width (the owner's choice of
[`designs/chat-attachments`](../designs/chat-attachments/README.md)):

| Kind | Shown as | Component |
|---|---|---|
| PNG and JPEG | the picture itself: one at 16:10, or up to four tiles with the rest counted ("+3"); each opens the image viewer | `MessagePictures` |
| Voice message | the player and its folded Transcript | `VoicePlayer` |
| Uploaded audio | the same player with its name, size and download under it | `MessageFiles` |
| Any other file | one row: type tile, name, type and size; the row opens it, the arrow downloads | `MessageFiles` |

A person's pictures come first in their message, their words under them and their
files last; an agent's follow its reply. Where no viewer is available a picture is
listed as a file. Your own picture-only message shows its time on the picture. A
preview that cannot be drawn shows the picture's name and still opens. The voice
player's length counts while it plays ("0:05 / 0:38") and a chip steps 1×, 1.5×, 2×
for that message. In the message box, files waiting to send are previews and tiles
with their own ×, and a file being stored shows there as "Uploading…".

Server: previews are now 640 px on the long side instead of 256 (`src/media.rs`),
so a 322 px picture is sharp on dense screens. They are still PNG and not cached.

Checked: `web/tests/chat-attachments.test.mjs` (types, sizes and name splitting; a
row opens the file and downloads it; one, three, four, five and seven pictures; the
order inside your own and another person's message; the message box's previews,
tiles, remove and an upload in place), the updated voice player tests (speed chip,
counting length, uploaded audio), all 544 client tests, type checking, the
production build, the media tests, and a browser render of the real components at
1100 px light and dark and 380 px with no sideways scroll.

Limits:

- A single picture is cropped to 16:10 and tiles to squares, because a message does
  not say how large its pictures are. Storing width and height at upload would let
  a picture keep its own shape with its space still held.
- A message being sent still shows as text with "N uploaded attachment(s)"; the
  outbox keeps file IDs only. Upload progress is not shown as a percentage.
- GIF, WebP and SVG are files, not pictures (the viewer shows PNG and JPEG only).
- Previews of a PNG photo are larger than before (640 px PNG, sent on every view).
- The forward window keeps its own small file list.

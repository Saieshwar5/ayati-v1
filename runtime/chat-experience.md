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

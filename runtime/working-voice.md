# The agent's working voice (schema 52)

An agent behaves like a colleague who gets a message: a quick thing is just
answered; something that takes time gets a "got it" first, a milestone or two
if it's long, and then the result. A request that queues behind the agent's
other work is answered at once by the agent itself, in its own words, and the
work follows. Built 2026-11-01 as the first slice of the asynchronous-agent
direction; working in chunks across attempts is the next.

Every reply is written by the model. The controller only decides when the
agent may speak and what it is allowed to see; it never writes the words.

## What people see

- **Acknowledgement.** When a request needs tools or more than one step, the
  agent posts one line saying what it will do, within seconds, and keeps
  working. Its reply still carries the result and the receipts.
- **Milestones.** For long work, an update when something real has happened:
  at most 12 per request and at least 45 seconds apart. In a channel or group
  the first update sits where the request was asked and later ones go into the
  thread under the request, so the channel isn't flooded; in a direct chat they
  stay inline. The final reply lands where replies always did.
- **The front desk.** When a person's request queues behind work the agent is
  already doing, a short model turn answers them within seconds, in the
  agent's voice and with its instructions: "Sure, Kavya, I'm in the middle of
  Ananya's September reconciliation; I'll pull Sharma's balance right after."
  It sees the recent conversation and a note of what the agent is doing. The
  note names the other person and their request only if the requester can
  see that conversation; otherwise it says "someone else" and tells the model
  not to say who or what. If the model can answer completely without tools
  ("What's your name?"), it answers and the queued work is dropped. Background
  work (schedules, triggers, hand-offs) never gets a front desk.
- **"Waiting to start" means waiting.** A request no machine has picked up
  shows "Waiting to start"; "typing…" appears only once the agent is actually
  at work. Waits that need a person keep their own labels.
- **Activity.** Each update and front-desk acknowledgement is a step on the request:
  "Said: Got it. I'll…". "See what it did" on an update opens that request.

Updates are the agent's own messages (`update_of` = the request's reply), never
"for" the requester, so they don't ring the requester's alerts; the final reply
and questions still do. They count as unread like any message.

## Manners per channel

Every agent has one gateway: built-in chat today, email and WhatsApp threads
through communications, calls later. How the agent behaves depends on where
the person wrote, and the controller enforces it.

| Where the person wrote | Acknowledgement and updates | Reply |
| --- | --- | --- |
| Built-in chat (direct, group, channel) | Front desk when busy; `send_update` while working | The result, when done |
| Email or WhatsApp thread | None; `send_update` returns `not_here` | One complete reply with everything |
| Calls (later) | Spoken acknowledgement, to be designed | Spoken answer or follow-up |

## How it works

- `send_update {text}` is an `ayati_control` tool. It needs the running claim,
  so an agent can only speak for the request it holds; after the reply is
  finished it's refused. Text is plain, 1–1000 characters. Over the limits it
  returns `limit` or `too_soon` without failing the task. In an email or
  WhatsApp thread it returns `not_here` and posts nothing.
- Instructions tell the agent when to use it and when not to (quick answers),
  not to promise durations it hasn't measured, and never to say it will
  continue later: until chunked work exists, the reply must finish the job.
- **Front desk.** Intake queues a `front_desk` executor task, in the same
  transaction as the request, only when all of these hold: the agent has
  other unfinished work (running with a live lease, or queued), the request
  is interactive built-in chat, execution is pooled, and the schema is 52+.
  A free agent gets none: its work lane starts at once and acknowledges
  itself with `send_update`, so a quiet agent costs no extra model call.
- The desk turn is tool-free, runs on a prepared slot for at most 30 seconds
  and needs no controller credentials. It may use capacity reserved for chat
  and is offered before other tasks, but only while its request is still
  waiting and the requester can still see the conversation and the agent.
- The model answers `{"reply": "...", "answer_now": true|false}`; fenced JSON
  is accepted and plain text counts as an acknowledgement. `answer_now` closes
  the request only if no machine has started it; the answer is published
  like any finished reply. Otherwise the text is posted as an update on the
  request. A desk turn that fails, times out or finds its request already
  moving says nothing.
- The work lane is told what the desk already said ("You already told the
  person, before starting: …"), so it doesn't repeat the acknowledgement.
- Migration 52 adds `relay_chat_messages.update_of` (a foreign key to the
  reply) and an index, the `relay_front_desk` record (state, decision
  `acknowledged|answered|nothing`, what was said), and the `front_desk` task
  purpose. The message projection includes `update_of` automatically.

## Checks

- PostgreSQL: `chat::updates_tests` (placement in channels, groups and direct
  chats; limits; refusal without the running claim; steps in Activity; the
  front desk for a visible conversation, a private one, more than one ahead,
  a free agent and background work; JSON, fenced and plain answers; an
  answer that closes the request and is published; a desk turn whose request
  was stopped first; no desk and no updates in an email thread) and
  `chat::front_desk::tests` (answer parsing).
- Guest: `send_update` forwards exactly one text.
- Web: `tests/working-voice.test.mjs` ("Waiting to start" versus typing; "See
  what it did" on an update opens its request; a reply reads after its
  updates) plus the existing reply tests.
- Journey: `tests/agent_working_voice.mjs` with real Goose: the acknowledgement
  arrives while the reply is still open; a second person's request gets the
  front desk's own words with no tools while the first request runs, shows
  "Waiting to start" while the first shows typing; the work lane is told what
  was said; both complete and Activity lists what was said.

## Next

Working in chunks (`continue_later`): the agent saves where it is, frees its
machine, and resumes as background work, so big jobs don't hit the attempt
limit and short requests from other people get a turn between chunks.

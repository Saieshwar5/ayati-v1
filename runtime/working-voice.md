# The agent's working voice (schema 52)

An agent behaves like a colleague who gets a message: a quick thing is just
answered; something that takes time gets a "got it" first, a milestone or two
if it's long, and then the result. A request that queues behind the agent's
other work hears so at once. Built 2026-11-01 as the first slice of the
asynchronous-agent direction; working in chunks across attempts is the next.

## What people see

- **Acknowledgement.** When a request needs tools or more than one step, the
  agent posts one line saying what it will do, within seconds, and keeps
  working. Its reply still carries the result and the receipts.
- **Milestones.** For long work, an update when something real has happened:
  at most 12 per request and at least 45 seconds apart. In a channel or group
  the first update sits where the request was asked and later ones go into the
  thread under the request, so the channel isn't flooded; in a direct chat they
  stay inline. The final reply lands where replies always did.
- **"I'm on another request."** When a person's request queues behind work the
  agent is already doing, the controller posts an instant reply in the agent's
  voice, with no model call: "I'm working on Meera's request at the moment;
  I'll start on this right after." It names the other person only if the
  requester can see that conversation; otherwise "another request". With more
  ahead: "I'll get to this after 2 others." The requester's own earlier
  request reads "I'm still working on your earlier request". Background work
  (schedules, triggers, hand-offs) never gets one.
- **"Waiting to start" means waiting.** A request no machine has picked up
  shows "Waiting to start"; "typing…" appears only once the agent is actually
  at work. Waits that need a person keep their own labels.
- **Activity.** Each update and busy reply is a step on the request:
  "Said: Got it. I'll…". "See what it did" on an update opens that request.

Updates are the agent's own messages (`update_of` = the request's reply), never
"for" the requester, so they don't ring the requester's alerts; the final reply
and questions still do. They count as unread like any message.

## How it works

- `send_update {text}` is an `ayati_control` tool. It needs the running claim,
  so an agent can only speak for the request it holds; after the reply is
  finished it's refused. Text is plain, 1–1000 characters. Over the limits it
  returns `limit` or `too_soon` without failing the task.
- Instructions tell the agent when to use it and when not to (quick answers),
  not to promise durations it hasn't measured, and never to say it will
  continue later: until chunked work exists, the reply must finish the job.
- The busy reply is written by intake, in the same transaction as the request,
  from the agent's other unfinished requests (running with an active lease, or
  queued). It runs only on schema 52+, so older schemas keep working.
- Migration 52 adds `relay_chat_messages.update_of` (a foreign key to the
  reply) and an index. The message projection includes it automatically.

## Checks

- PostgreSQL: `chat::updates_tests` (placement in channels, groups and direct
  chats; limits; refusal without the running claim; steps in Activity; busy
  replies for a visible conversation, a private one, the same person, more than
  one ahead, a free agent, and background work).
- Guest: `send_update` forwards exactly one text.
- Web: `tests/working-voice.test.mjs` ("Waiting to start" versus typing; "See
  what it did" on an update opens its request) plus the existing reply tests.
- Journey: `tests/agent_working_voice.mjs` with real Goose: the acknowledgement
  arrives while the reply is still open, a second person's request gets the
  busy reply with no model call and shows "Waiting to start" while the first
  shows typing, then both complete and Activity lists what was said.

## Next

Working in chunks (`continue_later`): the agent saves where it is, frees its
machine, and resumes as background work, so big jobs don't hit the attempt
limit and short requests from other people get a turn between chunks.

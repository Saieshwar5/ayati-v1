# Shared rooms: channel context (schema 55)

A **shared room** is a conversation more than one person reads: a public
channel, or a channel or group chat with two or more people. Everyone there
reads the agent's reply, so the room has its own context, separate from each
person's direct messages. Built 2026-11-01 (milestone G2 of the gateway plan,
D236).

Direct messages, a person's own group with agents, Library questions and
email/WhatsApp/transport threads have one reader and stay private, as before.

## One brief per room

The working summary an agent saves with `save_working_context` is the
**room's brief** in a shared room: one per agent per room, not one per person.
Everyone in the room reads the same goal, decisions, open questions and
corrections, and the Knowledge preview shows that brief to anyone who can open
the conversation.

- The brief is saved under the room (`relay_working_context.requester` is
  empty) and cites only messages and notes from that room's attempt, as before.
- Each agent in the room keeps its own brief.
- Two people's tasks race safely: the revision check lets the newer task win.
- Someone leaving the room, or the room becoming more private, fences the
  brief. The next task builds a new one from what the remaining audience may
  see.
- A person's direct conversation never reads a room's brief, and a room never
  reads a person's summary.
- Summaries saved per person in channels before schema 55 are no longer read
  there.

The agent sees it as "Shared brief for this conversation" and is told that
everyone in the room reads it.

## Who reads the reply

Every request in a shared room tells the agent who can read the reply, such as
"2 people" or "anyone in the workspace". It must use only what everyone there
may see and never put a person's private details in the room.

## Answering privately

When the answer needs the asker's own private information, such as their pay,
leave, personal details, private files or direct messages, the agent calls
`continue_privately` instead of answering in the room:

1. Cuckoo opens the asker's direct conversation with the agent, if there isn't
   one, and posts there, written by the agent: "You asked in Team: …" with the
   original question. Mentions become plain names.
2. A new request answers it there, for the same person, with that private
   conversation's context. The model sees its origin as
   `private_continuation`, with the asker as the initiating person.
3. In the room the agent replies with one short line, such as "I've answered
   you privately."

Rules:

- The tool takes no arguments: the agent can't choose the person, room or
  question.
- Only a person's own question can move. Handoffs, scheduled work and event
  work can't.
- It works only in shared rooms; in a private conversation it says to answer
  there.
- If the person may ask in the channel but can't message the agent directly,
  it is refused, and the agent says so without the private details.
- An exact retry returns the same private request.
- The moved request is a normal interactive request (no front desk), and
  `relay_private_continuations` records it.
- The private request does not read the channel's messages. A question that
  depends on earlier channel messages may need restating.

## Several agents asked at once

A message may ask up to three agents (schema 99,
[chat experience](chat-experience.md#several-agents-in-one-message-schema-99)). Each
answers in the room separately, with its own brief, and is told which others were asked
so it answers only its part. Each reads the room as it was when asked, without the other
agents' replies to that message. Moving to a private conversation is per agent: one
agent calling `continue_privately` moves the question for that agent only.

## Agents asking agents (D246)

When someone asks one agent to get something from another agent in the room, the
agents talk in the room (schema 100, [channel handoffs](channel-agent-handoffs.md#visible-in-the-conversation-schema-100-d246)):
the first agent posts its question to the other ("Nova, can you check the walnut
stock for Kapoor?"), the other answers below it, and the first agent's reply follows.
On the main timeline when the request was there, in its thread when it was in one.
Group rooms can hand off as channels can, with the same rules; direct messages and
email/WhatsApp threads can't. The question is written for everyone in the room, so it
leaves out anything they may not see, and each agent keeps its own permissions.

Later requests in the room remember the exchange when no part of it used source data
(an event, a business operation, Library documents, changed notes); otherwise the
whole exchange stays out of later context, like a direct reply built from a tool
([later memory](channel-agent-handoffs.md#later-memory-d246)).

## Threads

A request in a thread reads that thread first (up to 50 messages), then the
latest 10 messages of the main timeline. Other threads stay out. A request on
the main timeline still reads the last 60 messages of the whole conversation.
History search and reading still cover the whole conversation.

## Not built yet

- **Actions that return the asker's own data:** "my payslip", for example. They
  will refuse in shared rooms and move to the private room (milestone G4,
  actions bound to the asker).
- **A screen showing the brief in the channel itself.** It needs an approved
  design; the Knowledge preview shows it today.

## Checks

- PostgreSQL: `shared_rooms::tests`. They cover the room audience, one brief
  read by another member and fenced when someone leaves, the owner's private
  group staying private, thread-first reading, and moving a question: the
  direct room, the agent-written message, origin, exact retry, refusal in a
  private room and for someone who can't message the agent directly.
- PostgreSQL: `handoff_admission::visibility_tests` cover the exchange on the main
  timeline and in a thread and group rooms and their eligibility;
  `conversation_context::handoff_memory_tests` what later requests remember.
- Unit: the executor's `continue_privately` forwards no arguments.
- Journey: `tests/channel_context.mjs`, with real Goose. The owner saves a
  brief in a channel, another member's request reads it and knows two people
  read replies, and a salary question asked in the channel is answered only in
  the member's direct conversation; the channel shows only "I've answered you
  privately."

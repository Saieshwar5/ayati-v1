# Threads in channels (schema 123)

Built 2026-11-16, suggestions 1 to 7 of the channel improvements. Channels stay
readable, people hear about the threads they take part in, and agents keep a
conversation going in a thread without being mentioned every time. The help
desk mode (suggestion 8) is planned separately (Drive: proj-docs, "Help Desk
Channels Plan").

## Where agents answer

In a channel or group that several people read (public, or two or more people),
an agent asked in the main timeline answers **in a thread under the asking
message**. The channel shows the question with a thread line under it.

- Asked inside a thread: it answers in that thread, as before.
- Channel managers (its creator and admins) choose **Agents answer: In a thread
  under the question / In the channel** in the channel's settings
  (`relay_chats.agent_replies`, `thread` by default for every channel, including
  existing ones; `agent_replies` in `POST …/chats/{chat}/settings`).
- Unchanged: direct chats, a person's own group with agents, work rooms, and
  requests from schedules, events, email and hand-offs, which keep their places.
- A question asked in the main timeline reads the conversation as before; a
  follow-up in the thread reads that thread first, then the latest main
  messages. Updates, the front-desk line, approvals, hand-offs and a retry of a
  failed answer stay in that thread.

## The thread button and line

- A message's toolbar has **↩ Reply** (quote it in the conversation) and a
  visible **Reply in thread** button; it is no longer hidden in ⋯.
- Under a message with a thread: the faces of who took part (up to three), the
  reply count, "last reply 5 min ago", and "Ledger is working…" while an agent
  works there. It opens the thread.

## Continuing with the agent

In the thread panel, a reply that names nobody goes to **the agent a person last
asked in that thread**: the last agent a person @mentioned in the thread or its
first message, or the agent that started it. A helper an agent asked never takes
over. Naming anyone (a person or an agent) sends the reply only to whom it names.
A line above the box says where a reply goes: "Replies go to Ledger · @mention
someone to change". The server still checks that the person may ask that agent.

## Following

A person follows a thread they started, wrote in, were mentioned in, or asked an
agent in (`relay_thread_followed`). A reply there by someone else is "for them"
(`for_me` and the unread "mentions" count), so it alerts them under the
Mentions level and counts on the channel's badge. The thread panel shows **Stop
following** or **Follow**; that choice is stored (`relay_thread_follows`) and
wins. `GET/POST …/chats/{chat}/threads/{thread}/follow` with `{following}`;
repeating a choice changes nothing; only people who can read the conversation
may, and only for a thread's first message.

## What agents are told about the channel

With every request in a channel or group, the agent gets "This conversation:
#orders, a private channel with 6 people." and, when set, the channel's
description as its managers' words, marked as context, never instructions or
permission (300 characters). Direct chats and work rooms get nothing extra.

## The rule line

Channels and groups with agents say how to get their help at the top: "Ledger
answers when you @mention it, in a thread under your message. Reply in that
thread to keep going." (or "… answers in the channel when you @mention it.").

## Checks

- PostgreSQL `channel_threads::tests`: a main-channel request answered in a thread
  under it, a thread request in its thread, "in the channel" honoured and only
  managers may choose it, a one-person group and a direct chat unchanged, the
  channel purpose, following by starting a thread, stop and follow again,
  people outside the channel refused.
- Web: `navigation.test.mjs` (thread line, plain replies to the asked agent, a
  person mention asks nobody, Stop following), `chat-details.test.mjs` (the
  setting), `agent-chat-note.test.mjs` (the rule line).

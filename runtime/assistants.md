# Assistants: every member's own agent (schema 127)

Built 2026-11-17, the first piece of the personal assistant plan. Agents work
for the team; **your assistant works for you**. Concept screens for the whole
plan: "Your Assistant in Cuckoo" (2026-11-17).

## What is built

- **One per member.** The first time a member opens Chat or Agents, Cuckoo makes
  their assistant and their direct chat with it. It is named **Ayati** until they
  rename it (⋯ › Name, picture and role…), uses the workspace's default model,
  and has its own computer and browser where this Cuckoo has computers.
- **Private to its owner.** Only its owner can talk to it. It can't be opened to
  other people, has no other owners (the Owners menu and link are hidden), joins
  no channel and has no email address. The server refuses each of these.
- **Admins see its work**, as for any agent: its requests show in Activity for
  its owner and admins, and opening one from a chat they are not in is recorded
  in the audit log ([activity](activity.md#who-sees-what)). Admins can't talk
  to someone else's assistant.
- **First in Chat.** Your assistant's chat sits in its own section, "Your
  assistant", above Channels, and is not repeated under Direct messages. Its
  chat says: "Ayati is your assistant. Only you can talk to it. You and
  workspace admins can see its work."
- **No new "Only me" agents.** New agents are for everyone or selected people.
  An agent saved as Only me before keeps working and keeps that choice when
  edited. Saving a new agent as Only me is refused by the agents route.
- **In directories.** The chat directory and the Agents list mark an assistant
  (`assistant: true`); Agents shows "Your assistant", or for admins "<owner>'s
  assistant". Other members never see someone else's assistant.
- **When its owner leaves** the workspace, the existing rule applies: an agent
  with no owner is paused and takes no work; its record stays for admins.

## One room (second piece)

- **Schedules and triggers run in its room.** An assistant's schedule or event
  trigger never opens a work room. Its first run adds one line to the room
  ("Schedule: Delivery review", "Trigger: New mail"), and every run is a thread
  under that line (`relay_assistant_threads`): the request and the assistant's
  reply. A run reads the room and the earlier runs. Its Memory setting is not
  used, because there is one room. Triggers with per-subject memory keep their
  subject rooms.
- **It remembers what its tools told it.** For other agents, a reply built from
  a tool call or an event is left out of later context, because being in a
  conversation is not permission to reuse a source. In an assistant's own room
  its owner is the only reader, so those replies stay in its memory
  (`assistants::own_room`). Library-source answers, withdrawn notes and Home
  panel updates keep their rules.

To have your assistant work with another agent, use a thread (below). The
older way, mentioning an agent in your assistant's chat to invite it into a
thread there, was removed with schema 130.

## Threads (schema 128)

A thread is the permission you give your assistant to talk to **one agent or one
person for you about one matter**, until you stop it.

- **Your assistant only suggests a thread** (`suggest_thread`: who, a short title
  and the opening message). The suggestion shows at the top of your chat with
  your assistant and in the Threads window. Nothing is sent until you press
  **Start thread**. You can also write a thread yourself with **New thread**.
- **Every message is yours.** The opening message and every answer your
  assistant sends (`thread_reply`) are ordinary messages from you in your direct
  chat with that agent or person. They read them as written by you. Only you see
  the line "Written by Ayati · <title>" under such a message
  (`relay_chat_messages.assisted`, returned to its author only); admins have the
  same record in the database and the Audit log lines below.
- **It runs on its own.** While a thread runs, each answer from the other side
  is handed to your assistant (`dispatch_threads` in the scheduler loop, woken by
  the answer), one run at a time, as a thread named "Thread: <title> · <name>"
  in your chat with it. That thread holds its notes and what it tells you. You can
  write in the chat with the other side yourself at any time.
- **Stop, resume, delete.** A banner at the top of the chat where a thread runs
  says so and has **Stop**. A stopped thread sends nothing; **Resume** hands
  your assistant whatever arrived meanwhile. **Delete** removes the thread and
  your assistant's notes about it; the messages already sent stay, as yours.
  Your assistant ends a thread itself with `finish_thread` when the matter is
  settled.
- **Where threads show:** under your assistant in the chat list (each running
  thread opens the chat it runs in), **Threads** there opens the window with
  suggested, running and stopped threads.
- **Limits:** one running thread per chat; a thread is never with an assistant
  or with yourself; your assistant can only write in your threads, only while
  they run; after 30 sent messages a thread stops and waits for you; the same
  key sends once. Start, stop, resume and delete are written to the Audit log.
- Its guidance tells it to write as you, never say an assistant wrote it, share
  only what the matter needs, and ask you instead of sending when it needs your
  decision or information you did not agree to share.

`GET/POST /api/identity/workspaces/{w}/assistant/threads` (`{id, kind, counterpart,
title, opening}` creates and starts) and `POST …/assistant/threads/{id}/state`
with `{state: "running"|"stopped"|"deleted"}`. Tables `relay_work_threads` and
`relay_work_thread_sends`.

## Drafts, notes about you and regular checks (schema 129)

- **Draft with Ayati.** Above the message box of every conversation except
  your assistant's own, **Draft with <name>** asks your assistant for a reply.
  What you have typed is sent along as "what I want to say". Your assistant
  reads the last 20 messages you can read there (without the restricted
  sources that agents never keep) and writes the draft in a "Drafts" thread in
  its own room. **Nothing is posted in the conversation.** The draft shows above
  the message box, to you only; **Use draft** puts it in the box, and you edit
  and send it yourself. The message is yours; only you see "Written by <name> ·
  Draft" under it, and an Audit line records that a drafted message was sent.
- **What it knows about you.** Two short texts, given to your assistant with
  every task: *about you* and *how you write*. Your assistant adds one line at a
  time with `remember_about_person` (kept once, never secrets); you read and
  edit both under **About you & checks** below your assistant in the chat list.
- **Regular checks.** Off until you turn it on there, with how often (30 minutes
  to once a day) and an optional checklist. At each check the controller looks
  in code for messages people sent you directly or with an @mention since the
  last check. **Nothing new and no checklist: no run and no line in the room.**
  Otherwise your assistant runs once, in one "Regular check" thread in its
  room, and tells you what needs you and what it suggests. It answers nobody.

`GET/POST …/assistant/settings` (`{about, style, checks, every_minutes,
checklist}`), `POST …/assistant/drafts` (`{id, chat, thread, instruction}` returns
`{chat, reply}`), `GET …/assistant/drafts/{reply}` (`{state, text}`) and
`POST …/assistant/drafts/{reply}/used` (`{message}`). Table
`relay_assistant_settings`.

## How it is made

`relay_assistants(workspace, user_id, agent)` holds one row per member. The row
is written first and decides the agent id, so two requests at once make one
assistant; saving the same new agent twice is already an exact retry. The chats
list, the chat directory and the agents list call `assistants::ensure` for the
signed-in member. A failure (for example a default model the workspace no
longer allows) is logged, never blocks the page, and is tried again on the next
visit.

## Checks

- PostgreSQL `assistants::tests`: one assistant from two requests at once and a
  later one, a second member's own, the name, audience and flags, one direct
  chat, another member unable to talk to it or see it; renaming, and refusals
  for opening it to others, adding an owner, an email address and a channel.
- PostgreSQL `assistants::tests`, second piece: a schedule's runs as one thread in
  the room with no work room, the second run reading the first; a tool-built
  reply remembered by an assistant and still left out for another agent.
- Web `tests/navigation.test.mjs` (the first section, not repeated under Direct
  messages, the chat's line) and `tests/assistant-people.test.mjs` (no Only me
  for a new agent; kept for one saved that way).

- PostgreSQL `assistants::threads::tests`: a suggestion that sends nothing, the
  opening as the member's message asking the agent, one hand-over per answer and
  one run at a time, a retried send, stop, resume, finish and delete with the
  notes removed and the Audit lines; the record hidden from the other person,
  one running thread per chat, refusals for someone else's thread or assistant,
  and the 30-message stop.
- Guest `thread_schema` and web `tests/assistant-threads.test.mjs` (the
  suggestion and one start, stop/resume/delete, a new thread kept on failure,
  the mark only on your own message).

- PostgreSQL `assistants::settings::tests`: notes given with every task and
  each person's own, a line kept once, a draft asked once with nothing posted,
  refused for a chat you can't read, read only by its person, the record on
  your sent message only; a check that does not run when nothing is new, runs
  once for a new direct message, one at a time, and always with a checklist.
- Web `tests/assistant-draft.test.mjs` (one request, shown before use, used only
  when chosen; settings kept on a failed save).

## Not built yet

- A "never share without asking" list enforced by the server (today it is
  guidance to the assistant).
- Private suggestions under single messages and an "away" list; reading mail
  and calendar in a check (a check reads direct messages and mentions only).
- A morning brief and end-of-day wrap at set hours (checks run on an interval).
- An admin screen for who wrote each message, and admin switches to turn
  assistants, drafts or threads with people off for a workspace. Admins have
  the Audit log lines and the assistant's work in Activity.
- Threads with more than one other side; forgetting what a disconnected
  account told the assistant.

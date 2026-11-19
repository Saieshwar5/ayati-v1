# Assistants: every member's own agent (schemas 127 to 137)

Built 2026-11-17, the first piece of the personal assistant plan. Agents work
for the team; **your assistant works for you**. Concept screens for the whole
plan: "Your Assistant in Cuckoo" (2026-11-17).

## What is built

- **One per member.** The first time a member opens Chat or Agents, Cuckoo makes
  their assistant and their direct chat with it. It is named **Ayati** until they
  rename it (⋯ › Name, picture and role…), uses the workspace's default model,
  and has its own computer and browser where this Cuckoo has computers.
- **Private to its owner.** Only its owner can talk to it. It can't be opened to
  other people, has no other owners (the Owners menu and link are hidden), and
  joins no channel. The server refuses each of these.
- **Admins see its work**, as for any agent: its requests show in Activity for
  its owner and admins, and opening one from a chat they are not in is recorded
  in the audit log ([activity](activity.md#who-sees-what)). Admins can't talk
  to someone else's assistant.
- **First in Chat.** Your assistant's chat is the first row of the chat list,
  above Channels, with no heading and nothing under it (2026-11-19), and is not
  repeated under Direct messages. Its chat says: "Ayati is your assistant. Only you can talk to it. You and
  workspace admins can see its work."
- **No new "Only me" agents.** New agents are for everyone or selected people.
  An agent saved as Only me before keeps working and keeps that choice when
  edited. Saving a new agent as Only me is refused by the agents route.
- **In directories.** The chat directory and the Agents list mark an assistant
  (`assistant: true`); Agents shows "Your assistant", or for admins "<owner>'s
  assistant". Other members never see someone else's assistant.
- **When its owner leaves** the workspace, the existing rule applies: an agent
  with no owner is paused and takes no work; its record stays for admins.

## Set up on its agent page (2026-11-19)

An assistant is set up where every agent is: its page in Agents. The separate
**Assistant settings** window in Chat is gone; the sliders button in the header
of your chat with it now opens that page.

- **About you** is the assistant's name for the Instructions tab (an admin
  looking at someone else's assistant sees it as **Rules and skills**):
  - *What it knows about you* and *How you write*, shown to its person only.
    The texts belong to the member, not the agent, so an admin never reads
    them and the page does not ask for them on someone else's assistant.
  - **Rules** in place of the instructions box: short lines (up to 300
    characters, at most 40), added and removed one at a time, each saved at
    once. They are kept as the agent's `settings.instructions`, one rule per
    line, so they reach the assistant as its instructions and saving follows
    the profile's revision rule. Instructions written before show as one rule
    per line. Only a person adds a rule: the assistant has no tool that
    writes its own rules, because something it read could then set a standing
    instruction.
  - **Skills** and the documents it always reads, as for any agent. *Change
    with <name>* proposes skills only. Approved notes, lessons from shared
    work and channel history are not shown: an assistant joins no channel.
- **Automations** starts with **Regular checks** for its person, above its
  schedules and triggers.
- **Connections** and **Settings** (apps, MCP servers, computer, model, usage
  and budgets) are the ordinary tabs, unchanged.
- **Overview** says who it works for and counts its rules.
- `POST …/assistant/settings` now takes any subset of `{about, style, checks,
  every_minutes, checklist}` and keeps the fields left out, so the notes and
  the checks save separately and neither overwrites the other.

Checks: PostgreSQL `an_assistant_keeps_notes_its_person_can_edit_and_drafts_without_posting`
(a save of some fields keeps the others) and web `tests/assistant-page.test.mjs`
(notes and checks each send only their own fields and keep a failed draft;
a rule added once despite a double submit, kept on a failed save, removed; the
tab's name; an admin sees no notes and the page never reads them). Not yet
checked in a live browser.

## Learning from your edits (schema 137)

Built 2026-11-19. The first way an assistant gets better on its own.

- **The signal.** When you send a message your assistant drafted and the text
  differs from its draft (more than spacing), the assistant is shown both
  texts once. A draft sent as written teaches nothing and starts no run.
- **What it does.** One run in a "What I learned from your edits" thread in
  its room, as background work. It may keep up to two short lines with
  `remember_about_person` (how you write, or something lasting about you) and
  answers in one sentence what it kept, so you can see why a line appeared.
  A one-off change of facts is to be ignored.
- **Yours to see and undo.** What it keeps lands in *What it knows about you*
  and *How you write* on its page, where you edit or remove any line. Unlike a
  team agent's [lessons](agent-learning.md), nothing waits for approval,
  because the notes reach only your own assistant. They are given to it as
  information, never instructions; rules are only ever added by you.
- **Switch.** *Learn from my edits* under About you, on by default
  (`relay_assistant_settings.learn`, sent as `learn` to `…/assistant/settings`).
- **Never in the way.** The private record on your sent message is saved
  first. A paused assistant, a full queue or a used-up budget means that edit
  is not learned from; nothing is retried.
- Each text is cut to 2,000 characters. Each learned edit is one model run
  against the assistant's budget.

`POST …/assistant/drafts/{reply}/used` is unchanged for the client. Apply
`m20261119_000137_assistant_learning` (one column) with the administrator
migration procedure before starting this version.

Checks: PostgreSQL `an_edited_draft_teaches_the_assistant_how_its_person_writes`
(one run for an edited draft holding both texts, under its own line in the
room; once per edit; the kept line readable in settings; no run when switched
off) and, in the notes test, no run for a draft sent as written. Not yet
checked with a real model: whether its lines are good is unproven.

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
  your assistant. Nothing is sent until you press **Start**. You start one
  yourself by [handing a chat over from a message](#handing-a-chat-over-from-a-message-schema-136).
- **Every message is yours.** The opening message and every answer your
  assistant sends (`thread_reply`) are ordinary messages from you in your direct
  chat with that agent or person. They read them as written by you. Only you see
  your assistant's mark and name beside the time of such a message (its title
  on hover)
  (`relay_chat_messages.assisted`, returned to its author only); admins have the
  same record in the database and the Audit log lines below.
- **It runs on its own.** While a thread runs, each answer from the other side
  is handed to your assistant (`dispatch_threads` in the scheduler loop, woken by
  the answer), one run at a time, as a thread named "Thread: <title> · <name>"
  in your chat with it. That thread holds its notes and what it tells you. You can
  write in the chat with the other side yourself at any time.
- **Stop, resume, delete.** A pill in the header of the chat where a thread runs
  says "<name> is replying" (or "is writing…") and has **Stop**; pressing its
  words shows what you asked and how many messages it sent. A stopped thread
  shows "<name> stopped" with **Resume** and **Delete** and sends nothing; **Resume** hands
  your assistant whatever arrived meanwhile. **Delete** removes the thread and
  your assistant's notes about it; the messages already sent stay, as yours.
  Your assistant ends a thread itself with `finish_thread` when the matter is
  settled.
- **Where threads show:** a green mark on that chat's row in the chat list
  (grey once stopped), the pill in that chat's header, and a strip at the top
  of your assistant's own chat listing every chat it replies in, with Open,
  Stop, Resume and Delete. The Threads window and the rows under your
  assistant were removed on 2026-11-19; "thread" is no longer a word in the
  interface, where it means replies under a message.
- **Limits:** one running thread per chat; a thread is never with an assistant
  or with yourself; your assistant can only write in your threads, only while
  they run; after 30 sent messages a thread stops and waits for you; the same
  key sends once. Start, stop, resume and delete are written to the Audit log.
- Its guidance tells it to write as you, never say an assistant wrote it, share
  only what the matter needs, and ask you instead of sending when it needs your
  decision or information you did not agree to share.

`GET/POST /api/identity/workspaces/{w}/assistant/threads` (`{id, kind, counterpart,
title, opening}` creates and starts; the web client no longer calls it) and `POST …/assistant/threads/{id}/state`
with `{state: "running"|"stopped"|"deleted"}`. Tables `relay_work_threads` and
`relay_work_thread_sends`.

## Handing a chat over from a message (schema 136)

Built 2026-11-19. The earlier way to start a thread yourself (a form asking who,
a title and a first message you wrote) is gone from the interface.

- **From a message.** In a direct chat with one person or one agent, each
  message the other side sent has your assistant's mark on its toolbar: "Let
  <name> take over from here". It is not offered on your own messages, in
  channels, groups, your assistant's own chat, archived chats or while a thread
  already runs in that chat.
- **One instruction.** A small window quotes the message and asks "What should
  <name> do?" (up to 1000 characters). No title, no first message.
- **The assistant writes the first reply.** The thread is saved running in that
  chat with `instruction` and `anchor` (the chosen message). Nothing is sent for
  you first. `handled` is set just before the anchor, so the scheduler hands
  the assistant that message and everything after it at once, with "What your
  person asked you to do there: …". Its first `thread_reply` goes out as yours
  without you reading it first; Stop, the 30-message limit and the Audit log
  line ("Handed the chat with <name> to their assistant: …") are the controls.
- **In the conversation** a thin line, "<name> took over from here", sits above
  the anchor message. Only you see it.
- The title used in lists, the Audit log and the mark on sent messages is the
  instruction on one line, cut to 80 characters.

`POST …/assistant/take-over` with `{id, chat, message, instruction}` returns
`{chat}`; the same ID starts once. The server derives the other side from the
chat and refuses a message that is not theirs, a chat that is not a two-sided
direct chat you can read, another member's assistant, and a second running
thread in the chat. Thread listings add `instruction`, `anchor` and
`created_ms`. Apply `m20261119_000136_assistant_thread_takeover` (two columns on
`relay_work_threads`) with the administrator migration procedure before
starting this version; startup never applies migrations.

## Drafts, notes about you and regular checks (schema 129)

- **Draft with Ayati.** In the message box of every conversation except your
  assistant's own, the assistant's mark beside Attach (**Draft with <name>**)
  asks your assistant for a reply. What you have typed is sent along as "what I
  want to say". Your assistant reads the last 20 messages you can read there
  (without the restricted sources that agents never keep) and writes the draft
  in a "Drafts" thread in its own room. **Nothing is posted in the
  conversation.** A line on top of the message box says "<name> is writing a
  draft…" with Cancel; the finished draft is written into the box (since
  2026-11-19, replacing the separate card and **Use draft**), with "Draft from
  <name> · Only you see this. Edit it, then send." and **Discard**, which puts
  back what you had typed. You edit and send it yourself. The message is yours;
  only you see your assistant's mark beside its time, and an Audit line records
  that a drafted message was sent.
- **What it knows about you.** Two short texts, given to your assistant with
  every task: *about you* and *how you write*. Your assistant adds one line at a
  time with `remember_about_person` (kept once, never secrets); you read and
  edit both on your assistant's page in Agents, under **About you**.
- **Regular checks.** Off until you turn it on under **Automations** on that
  page, with how often (30 minutes
  to once a day) and an optional checklist. At each check the controller looks
  in code for messages people sent you directly or with an @mention since the
  last check. **Nothing new and no checklist: no run and no line in the room.**
  Otherwise your assistant runs once, in one "Regular check" thread in its
  room, and tells you what needs you and what it suggests. It answers nobody.

`GET/POST …/assistant/settings` (`{about, style, checks, every_minutes,
checklist}`; a save may send any of them), `POST …/assistant/drafts` (`{id, chat, thread, instruction}` returns
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
  for opening it to others, adding an owner and a channel.
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
  suggestion and one start; the header pill, list mark and strip with
  stop/resume/delete; a hand-over offered only on the other side's message,
  kept with its ID on failure; the mark only on your own message).
- PostgreSQL `a_chat_handed_over_from_a_message_starts_with_the_assistants_own_reply`:
  refusals (your own message, no instruction, a missing message, a chat you
  can't read), nothing sent at the start, one start for a retried ID, one
  running thread per chat, the instruction and anchor message handed to the
  assistant, its reply sent as the member, and resume without an opening.

- PostgreSQL `assistants::settings::tests`: notes given with every task and
  each person's own, a line kept once, a draft asked once with nothing posted,
  refused for a chat you can't read, read only by its person, the record on
  your sent message only; a check that does not run when nothing is new, runs
  once for a new direct message, one at a time, and always with a checklist.
- Web `tests/assistant-draft.test.mjs` (one request, your words kept while it
  writes, the draft written into the box, Discard restoring them).

## Not built yet

- Learning from more than draft edits: a chat you stopped it in, corrections
  in its own chat, and proposing a skill when you ask for the same kind of
  work again.
- Hiding what an assistant read in its person's own apps (mail, calendar)
  from admins in Activity. Today admins see an assistant's work like any
  agent's.
- Schedules on weekdays only, or that keep their local time across daylight
  saving. Every day and every week from a first run already work.

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

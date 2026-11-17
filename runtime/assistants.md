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

Mentioning another agent in your assistant's chat still works as before
([invited agents](forwarding-and-invited-agents.md)); it will be replaced by
work threads.

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

## Not built yet

A setup page, runs that stay quiet when nothing changed, forgetting what a
disconnected account told it, the "about you" memory, asking it from any message box, suggestions,
work threads, regular checks, and the admin switches.

# Assistants: every member's own agent (schemas 127 to 141)

> **2026-11-21:** what an assistant does in a chat is now one switch in that
> chat's header ([one position per chat](#one-position-per-chat-schema-141)).
> Handing a chat over from a message, and Stop, Resume and Delete, are gone.

> **2026-11-21:** an agent's page has no tabs any more
> ([agents workspace](agents-team-workspace.md)). Where this page says
> "Instructions tab" or "About you tab", read: the **Does** row (an assistant's
> is **Rules**, its short list in place), **Ask <assistant>** in that row for
> the change-by-chat panel, **About you** in the **Knows** row for its memory,
> and **Regular checks** in the **On its own** row.

> **2026-11-21:** the Draft button's own behaviour is gone. You now ask your
> assistant by mentioning it in any message box; see
> [Asked from any message box](#asked-from-any-message-box-2026-11-21).

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
- **First in Chat.** Your assistant's chat is the first conversation in the chat list (under For you),
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

## Asked from any message box (2026-11-21)

Built 2026-11-21 from [`designs/assistant-mention`](../designs/assistant-mention/README.md).
It replaces **Draft with <name>** of schema 129. No migration.

- **Mention it and say what you want.** In the message box of any conversation
  or reply thread except your assistant's own chat, type `@` and choose your
  assistant, then what to do: "go through this month's sales and answer
  Karthik", or only "respond". It is the first row of the list ("Your assistant
  · Only you see what you ask") and is also found by typing `@assistant` or
  `@my`, whatever you named it. A mention holds the assistant's ID, so
  renaming it changes nothing, and nobody is offered someone else's assistant.
- **The box says it is private.** While the text mentions your assistant the
  box has a purple border, the line "Asking <name> · Only you see this. Nothing
  is sent in #sales." and the button reads **Ask <name>**. Team agents named in
  the same text are "Named only, not asked".
- **What you asked is never posted.** Sending asks your assistant
  (`POST …/assistant/drafts`) and clears the box; nothing is added to the
  conversation. Three things keep those words from being posted by mistake:
  the client sends such a text to the assistant only; a name typed by hand
  ("@tara tell him…", which is not a mention) shows "This would be sent in
  #sales as written. Did you mean to ask <name>?" with **Ask <name> instead**;
  and the server refuses a posted message that mentions the sender's own
  assistant without asking it ("This asks your assistant, so it was not
  posted…"). Asked outright anywhere but its own chat, it is refused as an
  agent that is not in the conversation.
- **A card above the message box**, for you alone. While it writes: "<name> is
  writing a reply for you… Only you see this. You can keep writing." with
  Cancel. Then **Draft from <name>** · "Will be sent as you in #sales" (or "to
  Karthik", "in this thread"), what you asked, the message it answers, and the
  text.
  - **Send as me** posts the text as your own message, once, and keeps the
    private record (`…/drafts/{reply}/used`): your assistant's mark beside its
    time for you alone, and the Audit log line.
  - **Edit** moves it into the box under "Draft from <name> · Only you see
    this. Edit it, then send."; Discard there puts back what you had typed.
    What you change before sending still [teaches it](#learning-from-your-edits-schema-137).
  - **Ask <name> to change it…** asks for a new draft from this one ("shorter").
  - **Discard** drops the card. The run is not cancelled; what it writes later
    is not used.
- **"Respond" alone.** With nothing asked beyond respond, reply or answer, the
  draft answers the newest message meant for you: in a direct chat the other
  side's latest; elsewhere the latest that mentions you or is marked for you,
  else the latest from anyone else. A message you are quoting is answered
  instead, and your sent reply quotes it. The card shows "Answering <who>: …".
- **When the conversation moves on.** Messages from others since you asked
  show "2 new messages since <name> wrote this." with **Update draft**, and
  Send as me stops being the main button.
- **It waits longer.** The draft is read every two seconds for two minutes,
  then every six, for ten minutes in all. A draft asked for is kept for an
  hour in this browser tab (session storage, per person, workspace,
  conversation and thread), so leaving the conversation and coming back shows
  it again.
- **The mark beside Attach** (**Ask <name>**) only puts the mention in the box.
- **Never sent without you.** No wording makes it send: "handle this" still
  gives a draft. [Handing a chat over](#handing-a-chat-over-from-a-message-schema-136)
  stays the separate action.

`POST …/assistant/drafts` takes `{id, chat, thread, instruction}` as before and
now also `message` (the one message to answer; it must be one the assistant
may be shown in that conversation or thread) and `previous` (an earlier draft
of yours to change; `instruction` then says how, and that draft must be
finished). The assistant is told "What I asked for: …" and to look up what it
needs with its tools first, or "Change the draft you wrote for me…".

Checks: PostgreSQL `assistants::settings::ask_tests` (the message to answer
named apart from the conversation and refused from a conversation its person
can't read; the instruction passed on; a draft changed only once written, only
with a change named and only by its person; nothing posted; a message that
mentions your assistant refused, without telling anyone else whose assistant
it is). Web `tests/assistant-ask-model.test.mjs` (what the box asks, which
message "respond" answers, a typed name, the @ list under another name, Ask
in place of Send) and `tests/assistant-ask.test.mjs` (asked once despite a
double submit with nothing sent to the conversation; sent once and recorded;
the answered message, a moved-on conversation and a change; a refused request
keeping your words; Edit and Discard; the typed-name warning; a draft kept
across leaving; nothing in the assistant's own chat). Real browser against a
real controller, `tests/assistant_ask.mjs`, with the simulation's scripted
reply. **Not run with a real model**: how good its replies are, and whether it
looks things up before writing, is unproven.

Limits: the card does not show what the assistant used to write the draft,
nor what it is doing while it writes; files attached to the box stay in the
box and are not part of what is asked; a draft is not kept across a reload
after the hour, or in another tab or device (it is in the Drafts thread in
your assistant's chat); a voice recording made while the box mentions your
assistant is refused by the server rather than asked.

## Helping set up an agent (schema 139)

Built 2026-11-19. The three create steps stay. In step 2 (What it does) and on
an agent's Instructions tab, the helper in the chat panel is now **your own
assistant** wherever it can work; the [skill writer](skills.md#written-in-chat)
stays as the fallback, and without either both are written by hand.

- **Who answers.** The panel shows your assistant's face and name ("Chat with
  Ayati", "Change with Ayati"), not the agent that does not exist yet. No
  operator setting is needed: it runs as your assistant, on its model and
  against its budget, and each turn shows in Activity like its other work.
- **Asked like a draft.** Each turn is your message in one **Agent setup**
  thread in your assistant's room ("Setting up Iris: …"). The setup so far
  (name, job title, apps, the workspace's skills, the ground rules and skills
  already there, the last eight messages of the panel) travels with the task
  as labelled data, not in the message. Looking back does not read this thread.
- **What it is told.** For that task only its instructions gain the setup
  guide (`skill_writer/guide.rs`, shown in the panel as "What Ayati is told
  while setting up"): write the drafts first from what you said and what it
  already knows about you, ask at most one question, do nothing else.
- **Handed back in parts.** `write_agent_setup {part, name?, description?,
  text?}`, one call per part: `ground_rules`, `skill`, `remove_skill`,
  `reuse_skill`, `needs_app`, `job_title` (only suggested when the agent has
  none), `used` (one line it relied on from its memory or the Library,
  shown as "What Ayati used" so you can check it) and, in chat only, `name`.
  At most 40 parts a turn.
- **Nothing is saved by it.** Parts are kept on the turn
  (`relay_agent_setups`). Reading a turn runs the skill writer's checks: valid
  names, free in the workspace, a SKILL.md that passes the upload checks. You
  review the drafts in the same boxes as before and save through the ordinary
  agent and skill routes: Create agent, or one proposal at a time on a saved
  agent. A part is kept only on the reply it was handed back with: no other
  task, and nobody else's assistant, can write on your turn.
- **Rules stay a person's.** When the agent being changed is someone's
  assistant, no ground rules are drafted, and one handed back anyway is left
  out with a note.
- **While it writes** your words show at once and the box stays free; the
  panel reads the turn every two seconds for up to three minutes. **Stop**
  ends the wait and a failed or stopped turn gives your words back. The run
  itself is not cancelled; what it writes later is not used.
- **Limits.** A message of up to 2,000 characters; skills of up to 24,000
  bytes of steps in all; a part's text up to 6,000 characters.

`GET …/assistant/agent-setup` returns `{available, assistant: {id, name,
avatar}, guide}` (available when the workspace runs pooled executors and your
assistant exists and is not paused). `POST` with `{id, setup}` (`setup` is the
skill writer's request) returns `{chat, reply}`; the same id asks once.
`GET …/assistant/agent-setup/{reply}` returns `{state}` while it works, then
the writer's answer plus `job_title` and `used`. Apply
`m20261119_000139_agent_setups` (two tables: these turns, and the
[unfinished agent](agent-profiles.md#an-unfinished-agent-is-kept-schema-139)
New agent keeps) with the administrator migration procedure before starting
this version.

Checks: PostgreSQL `assistants::setup::tests` (availability; one turn asked
once in the Agent setup thread; the setup given to that task only; parts kept,
bad parts and other tasks or members refused; checked drafts read only by the
person who asked; nothing saved; only someone who may change a saved agent;
the size limits; no ground rules for an assistant), guest `setup_schema`, and
web `tests/assistant-agent-setup.test.mjs` (asked once despite a double click,
drafts filling the step, the suggested job title, what it used, the app note;
a refused, failed and stopped turn giving the words back; the timer cleared on
leaving; the writer as fallback; proposals on an agent's page applied by the
person). **Not run with a real model or a real executor**: how good its drafts
are, and how long a turn takes compared with the writer's single call, are
unmeasured.

Limits: a setup turn has all of the assistant's usual tools; it is told to do
nothing else, but a task is not yet limited to reading. A turn still being
written when the page is reloaded is not picked up again (it stays in the
Agent setup thread). After Create agent in New agent nothing yet lists the
apps the assistant said the agent needs.

### Asked for in your chat with it

Built 2026-11-19. You can ask your assistant for a new agent in your ordinary
chat with it ("I need an agent that chases unpaid invoices"), with no form.

- **It drafts on its reply.** Its instructions now say how: with
  `write_agent_setup` it hands back a `name`, a job title, ground rules,
  skills, the apps the job needs and what it used. The first part begins a
  draft on that reply. This works only in its own chat with its own person:
  not for another agent, in another conversation, or in the threads of its
  schedules, checks, drafts and setup turns.
- **A card under the reply, for you alone,** shows the whole draft: every
  ground rule, each skill with its steps, the apps and what it used. Nobody
  else gets the card, and nothing exists yet.
- **You create it.** **Create agent** on the card saves the agent and its
  skills through the ordinary routes, with you as its owner and everyone in
  the workspace able to talk to it; the card says so first. The agent's id and
  each skill's id come with the draft, so creating again after a failure
  updates the same agent and saves each skill once. Afterwards the card reads
  Created and links to its Connections tab and its page.
- **Or review it first.** **Review in New agent** opens the same draft in the
  three steps (`?view=agents&setup=<reply>`), where it is edited, kept and
  created as any new agent.
- **Changes carry on.** "Make it more formal" in a later message changes only
  those parts: for a day, until the agent exists, the next reply starts from
  the draft before it, under the same agent id, and is shown that draft with
  its request. The earlier card then reads "earlier draft, replaced by the
  one below", and Create on a card checks for a newer draft first.
- **What it can't do** is unchanged: it creates nothing, connects no apps and
  sets no controls.

A chat message carries `agent_setup: true` for the person the draft is for.
`GET …/assistant/agent-setup/{reply}` adds `from: "chat"`, `name`, `agent_id`,
`created`, `replaced` and an `id` on each skill.

Checks: PostgreSQL `assistants::setup::chat_tests` (a draft begun on the
reply, its card for its person only, stable ids, a change carrying on under
the same agent, a dropped skill, the earlier draft marked replaced, created
through the ordinary save, a new draft afterwards, nothing carried after a
day; refused for another agent and in the drafts thread) and web
`tests/agent-draft-card.test.mjs` (the whole draft shown, one save despite a
double click, a failed save kept and retried, the created state, nothing
while the reply is written, a replaced and an unnamed draft, the Agents
link). The card was looked at in a browser with canned answers. **Not run
with a real model**: whether the assistant reaches for the tool when asked,
and how good its drafts are, is unproven.

Limits: only new agents (to change a saved agent's skills, use its
Instructions tab); a card already on screen learns that it was replaced when
the page is read again or when Create is pressed; an agent created from the
card whose skills then fail to save shows as Created after a reload, with
the missing skills to add on its page.

## Memory (schema 138)

Built 2026-11-19. An assistant's long-term memory: what it remembers about
its person and their work across tasks. It replaces the two note texts of
schema 129. Design notes from reading Mem0, Letta, Graphiti, LangMem and
Anthropic's memory tool are at the end of this section.

- **One short line per memory** (up to 300 characters, at most 500 current
  lines a person), in five kinds: `about` (who they are), `style` (how they
  write), `ways` (how they want things done), `people` (who they work with)
  and `work` (lasting facts: projects, dates, decisions).
- **Added, never rewritten.** The assistant keeps a line with
  `remember_about_person {kind, text}` (the same words are kept once) and ends
  one that stopped being true with `forget_memory {memory}`, then keeps the
  new one. An ended line stays, unused, for its person to bring back or
  delete. A model never edits or erases a line.
- **In every task.** Current lines are given to the assistant with each
  request, steady kinds first (about, style, ways, people, work) and newest
  first, up to 6,000 characters, each as `[name] kind: text`. When more exist
  the text says how many, and the assistant finds them with
  `search_memory {query}` (any of the words, ranked, 15 lines). Since schema 142
  the same search also returns the workspace's memory, which the assistant reads
  with every task and never writes ([memory](memory.md)).
- **Information, never instructions.** The lines are labelled so. A kind that
  is not one of the five is refused, and only a person writes rules.
- **Where it learns from.** While working with its person (`chat`); from an
  edited draft and from looking back (`review`); and lines its person writes
  (`you`). Each line shows its source and date.
- **Looking back.** About every 20 hours the controller checks, in code, what
  the person and the assistant said in their own conversation since the last
  look (not the threads of its schedules, checks, drafts or earlier looks).
  Nothing from the person: no run. Otherwise one background run in the "What
  I learned" thread is shown up to 40 messages (600 characters each) and may
  keep up to five lines and end stale ones; it answers in one sentence. A
  paused assistant or a full queue is tried again an hour later.
- **Its person's alone.** They read, add, change, remove and bring back lines
  under **Memory** on the assistant's page. Removing deletes for good. Nobody
  else, admins included, has a route to them; another member's assistant
  finds nothing and can end nothing.
- **Switch.** *Keep learning on its own* (the `learn` setting of schema 137)
  turns off both the edit learning and looking back. The assistant still
  keeps what its person tells it to remember.

`GET …/assistant/memories` returns `{memories, ended, most}`; `POST` with
`{id, kind, text}` adds a line or changes the person's own (the same id saves
once); `POST …/assistant/memories/{id}/remove` and `…/restore`.
`…/assistant/settings` no longer carries `about` and `style`. Tables:
`relay_assistant_memories`; `relay_assistant_settings` gains `reflected`,
`reflect_next_ms` and `reflect_job`. Apply `m20261119_000138_assistant_memory`
with the administrator migration procedure before starting this version: it
copies each line of the old two texts into memory and leaves the old columns
unread.

Checks: PostgreSQL `assistants::memory::tests` (a person's line saved once
and private; a fact kept once by the assistant, given with every task and
found by a word; a wrong kind refused; another member's assistant finding and
ending nothing; a line ended, brought back, changed and removed; the overflow
note; looking back only when its person said something, one run at a time,
its line marked `review`, never at its own look, and off with learning),
guest `thread_schema`, and web `tests/assistant-page.test.mjs`. The page was
looked at in a browser against canned API answers. **Not run with a real
model**: whether it keeps the right things is unproven.

Why it is built this way, and not by adding one of those projects: they are
Python or TypeScript services with their own vector or graph store, and
would need Cuckoo's per-person access rules built a second time. From them:
a small always-present memory plus search (Letta); a background pass over
recent work (Letta, LangMem); add-only writes, which Mem0 moved to after
letting a model update and delete; ending a contradicted fact instead of
erasing it (Graphiti); and a recorded source for every line, kept apart from
instructions, against planted memories.

Limits: search is by words, not meaning (the Library's pgvector search is
not used yet); a line's source says who wrote it, not which message taught
it; nothing learned from connected apps is marked as less trusted; looking
back does not yet propose skills.

## Set up on its agent page (2026-11-19)

An assistant is set up where every agent is: its page in Agents. The separate
**Assistant settings** window in Chat is gone; the sliders button in the header
of your chat with it now opens that page.

- **About you** is the assistant's name for the Instructions tab (an admin
  looking at someone else's assistant sees it as **Rules and skills**):
  - **Memory** (above), shown to its person only. It belongs to the member,
    not the agent, so an admin never reads it and the page does not ask for
    it on someone else's assistant.
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
- `POST …/assistant/settings` takes any subset of `{checks, every_minutes,
  checklist, learn}` and keeps the fields left out, so each part of the page
  saves its own.

Checks: PostgreSQL `an_assistant_keeps_notes_its_person_can_edit_and_drafts_without_posting`
(a save of some fields keeps the others) and web `tests/assistant-page.test.mjs`
(checks send only their own fields; a rule added once despite a double
submit, kept on a failed save, removed; the tab's name; an admin sees no
memory and the page never reads it).

## Learning from your edits (schema 137)

Built 2026-11-19. The first way an assistant gets better on its own.

- **The signal.** When you send a message your assistant drafted and the text
  differs from its draft (more than spacing), the assistant is shown both
  texts once. A draft sent as written teaches nothing and starts no run.
- **What it does.** One run in the "What I learned" thread in its room, as
  background work. It may keep up to two short lines with
  `remember_about_person` (how you write, or something lasting about you) and
  answers in one sentence what it kept, so you can see why a line appeared.
  A one-off change of facts is to be ignored.
- **Yours to see and undo.** What it keeps lands in its [memory](#memory-schema-138),
  where you edit or remove any line. Unlike a
  team agent's [lessons](agent-learning.md), nothing waits for approval,
  because the notes reach only your own assistant. They are given to it as
  information, never instructions; rules are only ever added by you.
- **Switch.** *Keep learning on its own* under Memory, on by default
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

To have your assistant work with another agent, turn it on in your chat with
that agent (below). The older way, mentioning an agent in your assistant's chat
to invite it into a thread there, was removed with schema 130.

## One position per chat (schema 141)

Built 2026-11-21, from [`designs/assistant-replies`](../designs/assistant-replies/README.md).
The owner found threads too complicated to use: two ways to start, an
instruction each time, four states and five buttons.

- **One switch.** In a direct chat with one person or one agent (not an
  assistant, not archived), your assistant's mark in the header opens three
  choices: **Off**, **Ask me first** and **Reply for me**. While off it is a
  quiet mark; while on, a pill reads "<name> asks you first", "<name> is
  replying" or "<name> is writing…".
- **Ask me first.** Each time the other side writes, your assistant writes the
  reply in its own room, one run at a time. Nothing is posted for you. The
  reply shows in the draft card above your message box, like a draft you asked
  for ([asked from any message box](#asked-from-any-message-box-2026-11-21)):
  **Send as me**, **Edit**, **Discard**, and a line to ask for a change. Your
  message box stays free. You send it yourself: the message is yours, the
  private record is kept as for a draft, and an edited reply teaches your
  assistant as an edited draft does. A waiting reply goes when you answer
  yourself, discard it, or the other side writes again. A draft you asked for
  is shown first when both exist.
- **Reply for me.** What a thread did before: it sends as you. It starts only
  from **Start replying**, under an optional "What should <name> do?" and a line
  saying that replies go out without you reading them first. A thin line,
  "<name> replies for you from here", sits above the first message it answers.
- **It stops sending by itself.** When it calls `finish_thread`, or after 30
  sent messages, the chat goes to Ask me first and keeps the reason ("Finished:
  …", "Sent 30 messages here. It asks you first now."). Choosing Reply for me
  again starts the count again. A thread that cannot go on is turned off with
  the reason, shown in the header's menu and in your assistant's own chat,
  where **Dismiss** removes it.
- **It starts from what is unanswered.** A change of position hands over what
  the other side wrote since your own last message (the last 20 messages are
  shown to it, as before).
- **Where it shows.** The mark on the chat's row (green while it replies for
  you, grey on Ask me first, none while off), the header, and a strip in your
  assistant's own chat: "Working in N of your chats", each with **Open** and
  **Turn off**.
- **Suggestions are unchanged.** **Start** on a suggestion sends its opening as
  your message, and the thread replies for you.

### The @name tag on what it wrote

What your assistant wrote shows its name as a mention on a line at the top of
your own message, in place of the mark beside the time: "@Ayati sent this for
you" when it sent the message itself, "Written with @Ayati" when you pressed
Send (a draft, a waiting reply, or a suggested opening you started). Only its
author is given `assisted`, so nobody else sees the tag.

### A reply to one message

Asking your assistant by mention is described under
[asked from any message box](#asked-from-any-message-box-2026-11-21). On top of
it, **Reply with @<name>** on the toolbar of any message that is not yours
quotes that message and puts the mention in the box, ready for what to ask;
the draft answers the quoted message and stays a reply to it when sent. This
took the place of "take over from here".

`POST …/assistant/replies` with `{id, chat, mode: "off"|"ask"|"auto",
instruction?}` returns `{thread}`; `id` names the thread only when this turns
your assistant on in that chat for the first time, and a left-out instruction
keeps the earlier one. `POST …/assistant/threads/{id}/draft/discard` drops the
waiting reply. Thread listings add `mode` and `draft: {reply, state, text}`.
`POST …/assistant/take-over` is removed. `relay_work_threads` gains `mode` and
`draft`. Apply `m20261121_000141_assistant_reply_modes` with the administrator
migration procedure before starting this version.

Checks: PostgreSQL `assistants::threads::modes::tests` (refusals for an unknown
position, an assistant's own chat, a group, a long instruction and a chat you
are not in; nothing sent by turning it on; one thread per chat; the reply
waiting and refused as a send; sent as written with its record; dropped when
you answer yourself and when discarded, by you only; replies sent as you from the unanswered message with what you asked;
the count started again; the step down at the limit and when finished; off
handing nothing over; the Audit lines), the two `assistants::threads::tests`,
guest `thread_schema`, and web `tests/assistant-replies.test.mjs` and
`tests/assistant-threads.test.mjs` (one request despite a double click, a
refused start keeping your words and its ID, Reply for me only from its own
button, the waiting reply as a card that is sent as you with its record,
moved into the box, or discarded, a draft you asked for shown before it, a
reply started from the other side's message only, the marks, the strip and
the tag), and `tests/assistant_replies.mjs` against a real controller and
Chromium (the switch through all three positions, the tag on a draft you
send and none for the other side, and Reply with @name). That controller
answers with scripted replies and runs no executor, so the browser check does
not hand anything over on its own. **Not run with a real model**: how
good the waiting replies are, and whether the assistant keeps from calling
`thread_reply` on Ask me first without being refused, is unproven.

Limits: a waiting reply is shown only in that chat's message box and as "a
reply is ready" in your assistant's own chat; nothing in Activity says one is
waiting. When your assistant needs your decision while replying for you it
still says so in its own chat, not in the chat where the question came up.
There is no flag on a reply that shares something new, no starting position
for all chats with agents or with people, and a suggestion still sends its
opening on Start.

## Threads (schema 128)

A thread is the record behind a chat your assistant works in: **one agent or
one person, in your direct chat with them**. Since schema 141 you turn it on
and off with [the switch in that chat](#one-position-per-chat-schema-141); the
rules below about whose messages they are still hold.

- **Your assistant only suggests a thread** (`suggest_thread`: who, a short title
  and the opening message). The suggestion shows at the top of your chat with
  your assistant. Nothing is sent until you press **Start**. You turn your
  assistant on yourself with the switch in that chat's header.
- **Every message is yours.** The opening message and every answer your
  assistant sends (`thread_reply`) are ordinary messages from you in your direct
  chat with that agent or person. They read them as written by you. Only you see
  your assistant's name as a mention on top of such a message
  (`relay_chat_messages.assisted`, returned to its author only); admins have the
  same record in the database and the Audit log lines below.
- **It runs on its own.** While a thread runs, each answer from the other side
  is handed to your assistant (`dispatch_threads` in the scheduler loop, woken by
  the answer), one run at a time, as a thread named "Thread: <title> · <name>"
  in your chat with it. That thread holds its notes and what it tells you. You can
  write in the chat with the other side yourself at any time.
- **Turned off and on in that chat.** Off sends nothing and hands nothing
  over; turning it on again hands your assistant what arrived since your own
  last message. Your assistant says the matter is settled with `finish_thread`
  and stops sending. The Threads window and the rows under your assistant were
  removed on 2026-11-19; "thread" is not a word in the interface, where it
  means replies under a message.
- **Limits:** one running thread per chat; a thread is never with an assistant
  or with yourself; your assistant can only write in your threads, only while
  it replies for you there; after 30 sent messages it stops sending and asks
  you first; the same key sends once. Turning it on and off, and each change
  between Ask me first and Reply for me, are written to the Audit log.
- Its guidance tells it to write as you, never say an assistant wrote it, share
  only what the matter needs, and ask you instead of sending when it needs your
  decision or information you did not agree to share.

`GET/POST /api/identity/workspaces/{w}/assistant/threads` (`{id, kind, counterpart,
title, opening}` creates and starts; the web client no longer calls it) and `POST …/assistant/threads/{id}/state`
with `{state: "running"|"stopped"|"deleted"}` (`running` starts a suggestion,
`stopped` turns one off from your assistant's own chat, `deleted` dismisses
one). Tables `relay_work_threads` and `relay_work_thread_sends`.

## Handing a chat over from a message (schema 136)

Built 2026-11-19 and replaced on 2026-11-21 by
[one position per chat](#one-position-per-chat-schema-141). The window that
quoted a message and asked "What should <name> do?", the mark that opened it
and `POST …/assistant/take-over` are gone. The two columns it added to
`relay_work_threads` are still used: `instruction` is what you asked your
assistant to do there, and `anchor` is the first message it answers for you.
The mark on a message's toolbar now asks for one reply to that message.

## Since schema 149: the regular check is a task

Built 2026-11-24, step 5 of [one system](../designs/agent-tasks/one-system.md) (decision
7). Migration `m20261124_000149_check_tasks` gives every assistant whose checks are on a
task **"Regular check"** for its person (`origin='check'`,
`relay_assistant_settings.task`): a minute rule with the chosen interval, `precheck =
'messages'`, the checklist as `instructions`, in the person's chat with their assistant,
its next run kept from the old due time. Checks that are off get no task; turning them on
makes it.

- **The code check comes first.** When a run is due, Cuckoo looks in code for direct
  messages and mentions since the last check (`checked`). Nothing new and no checklist:
  the run is recorded as no change and no model is called. Otherwise the assistant runs
  in the "Regular check" thread, as before; when nothing needs the person it answers
  `NO_CHANGE` and nothing is shown (before: "Nothing new."). A check due while the last
  is still running is skipped, as for any task.
- **The panel edits the task.** Regular checks keeps on/off, how often and the checklist.
  Saving changes the task in the same transaction: off pauses it, on resumes it, a new
  interval counts from now, the checklist becomes its instructions. The panel shows the
  next check and links to the task. Pausing it on the task page shows checks as off;
  ending it there turns them off. The agent can pause it in chat but not change its times
  or checklist.
- **Removed:** the separate check dispatcher (`dispatch_checks`) and its place in the
  scheduler loop. `GET …/assistant/settings` adds `task` (next run, paused).

Checks: PostgreSQL `tasks::check_tests` (migration from schema 148, quiet runs without a
model call, a run in the check thread) and
`assistants::settings::tests::a_regular_check_runs_only_when_something_reached_its_person`
(on, off, interval, checklist, ending); web `assistant-page`.

## Drafts, notes about you and regular checks (schema 129)

- **Drafts.** Since 2026-11-21 a draft is asked for by mentioning your
  assistant in the message box and arrives in a card above it; see
  [Asked from any message box](#asked-from-any-message-box-2026-11-21). The
  assistant reads the last 20 messages you can read there (without the
  restricted sources that agents never keep) and writes the draft in a
  "Drafts" thread in its own room. **Nothing is posted in the conversation.**
  You send it yourself. The message is yours; only you see "Written
  with @<name>" on top of it, and an Audit line records that a drafted
  message was sent. (Before: a **Draft with <name>** button beside Attach sent what you
  had typed as "what I want to say", and the draft was written into the box.)
- **What it knows about you.** Until schema 138 two short texts, *about you*
  and *how you write*; now its [memory](#memory-schema-138).
- **Regular checks.** Off until you turn it on under **Automations** on that
  page, with how often (30 minutes
  to once a day) and an optional checklist. At each check the controller looks
  in code for messages people sent you directly or with an @mention since the
  last check. **Nothing new and no checklist: no run and no line in the room.**
  Otherwise your assistant runs once, in one "Regular check" thread in its
  room, and tells you what needs you and what it suggests. It answers nobody.

`GET/POST …/assistant/settings` (`{checks, every_minutes, checklist, learn}`;
a save may send any of them), `POST …/assistant/drafts` (`{id, chat, thread, instruction, message?, previous?}` returns
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
  one run at a time, a retried send, stop, resume, the step to Ask me first when
  finished and at 30 messages, delete with the notes removed and the Audit
  lines; the record hidden from the other person, one running thread per chat,
  and refusals for someone else's thread or assistant.
- The switch, the waiting reply, asking by mention and the tag: see
  [one position per chat](#one-position-per-chat-schema-141).

- PostgreSQL `assistants::settings::tests`: notes given with every task and
  each person's own, a line kept once, a draft asked once with nothing posted,
  refused for a chat you can't read, read only by its person, the record on
  your sent message only; a check that does not run when nothing is new, runs
  once for a new direct message, one at a time, and always with a checklist.
- Web `tests/assistant-ask.test.mjs`, which replaced `assistant-draft.test.mjs`
  on 2026-11-21.

## Not built yet

- Learning from a chat you stopped it in and from refused approvals, and
  proposing a skill when you ask for the same kind of work again.
- Memory search by meaning, a link from a line to the message that taught
  it, and lower trust for lines learned from connected apps.
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
- Your assistant's questions in the chat where they came up and in Activity →
  Needs you; a waiting reply shown outside its chat; a flag on a reply that
  shares something new; a starting position for all chats with agents or with
  people; a suggestion that opens as a reply in the message box.
- An admin screen for who wrote each message, and admin switches to turn
  assistants, drafts or threads with people off for a workspace. Admins have
  the Audit log lines and the assistant's work in Activity.
- Threads with more than one other side; forgetting what a disconnected
  account told the assistant.

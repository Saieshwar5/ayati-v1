# One system for everything agents do on their own (approved 2026-11-24)

The owner decided that schedules, event triggers and an assistant's regular checks
must not stay separate systems next to tasks. Everything an agent does on its own
is a **task**. This file is the build contract; [README.md](README.md) is the
product picture and [implementation-plan.md](implementation-plan.md) the first build.

## The picture

A task wakes in one of three ways:

| Wakes | Example | Stored as |
| --- | --- | --- |
| At a time | "Every Monday at 9, send the sales summary" | `relay_tasks.repeat` (calendar rule), runs in `relay_task_wakeups` |
| When something happens | "When a new order arrives in Zoho, make the invoice" | the trigger's reviewed setup in `relay_event_triggers`, linked to its task |
| Keep checking | "Every hour, tell me what needs me" | a rule plus a check in code before the model (`relay_tasks.precheck`) |

Follow-ups (`follow_up`) and long jobs (`continue_later`) are wake-ups of a task, as
in schema 144. Every task has the same rules (missed and overlapping runs, three
failures pause, give-up, notes, `NO_CHANGE`), the same page (Pause, Skip next, End),
and shows on the same Calendar and agent page.

## Decisions

1. **One rule (owner, 2026-11-25; replaces the channel rule of 2026-11-24).** "Even
   scheduled tasks should work similar to normal tasks; they can have entire channel
   context room access. Even events also work the same like schedules; it should be who
   activated the trigger." **A task run happens in the task's own conversation, as the
   person it belongs to**, exactly like that person asking the agent there:

   | What | Belongs to | Runs in | Access of |
   | --- | --- | --- | --- |
   | Task asked in a direct chat | the asker | that chat | the asker |
   | Task asked in a channel or group | the asker | that channel or group (its recent messages, threads and room memory shelf, like an @mention) | the asker |
   | Event trigger | the person who set it up | the conversation chosen when setting it up; none chosen: that person's direct chat with the agent | that person |
   | Assistant's regular check | its person | the person's chat with the assistant, "Regular check" thread | its person |

   Each run's input shows as one "↻ Following up on <task>" line (an event run: "↻
   <task>"), then the agent's reply; a `NO_CHANGE` run shows nothing. Approvals and the
   agent's questions go to the run's person as for their own background requests.
   Access is checked again at every run by the ordinary intake: someone who left the
   conversation or workspace blocks the wake-up. Work rooms, a trigger's Memory
   setting, per-subject rooms and copied run results are gone (schema 151). Before,
   the channel rule (2026-11-24) ran a channel's task in a private work room
   (`relay_tasks.room`) and posted each result to the channel.
2. **Instructions.** A task keeps what each run must do (`relay_tasks.instructions`,
   up to 10,000 characters). `create_task` and `change_task` take `instructions`; a
   converted schedule keeps its content there.
3. **Minute rules.** The rule gains `unit: "minute"` (`every` 15 to 1440): a fixed
   period counted from the task's first run, so every old interval converts exactly.
4. **Schedules become tasks (forward migration).** Each schedule that is not a Home
   panel update becomes a commitment task: title = name, instructions = content,
   for = its person, agent, time zone, first run as anchor, its rule (whole weeks →
   weekly on the first run's weekday and time; whole days → daily; whole hours →
   hourly at the same minute; otherwise minutes), and its next run booked when it is
   enabled. A paused or blocked schedule becomes a paused task with its reason; a
   completed one a done task. Since schema 151 it runs in the conversation it posted
   to, or with no post target in its person's direct chat with the agent (decision
   1). Its earlier runs are linked to the task. The old row is marked moved and
   no longer runs (schema 150 dropped the schedule tables).
5. **Home panel updates are not tasks.** A panel's fixed-interval update moves from
   its schedule binding to the panel's own update queue (`relay_home_update_wanted`),
   which already handles "after work" and "on an event". People see no change.
6. **Event triggers become task wake-ups.** Each trigger gets a task (`relay_event_
   triggers.task`): title = the trigger's name, instructions = its task text. A
   delivered event starts a run of that task (no separate automatic task per
   delivery). The trigger keeps its reviewed source, fields and filter and is still
   set up once from the agent page; pausing, resuming or ending the task pauses,
   resumes or ends the trigger, and the other way round.
7. **Regular checks become a task.** An assistant's regular check is a task "Regular
   check" for its person with a minute rule, `precheck = 'messages'` (the existing
   code check for messages to the person since the last run) and its checklist as
   instructions. The assistant page's Regular checks panel edits that task.
8. **Removed.** The schedule half of New automation, the schedule create/state/runs
   API, the schedule dispatcher and occurrence writing, schedule proposals in Chat
   and their cards, the regular-check dispatcher, Activity › Calendar's List view and
   the Planned/upcoming list, and the separate schedule list on the agent page.
   Calendar and task pages read tasks only.

## Order

Each step is its own schema version and commit, with its tests passing before the
next starts.

1. Task rooms, instructions, minute rules and the channel rule (replaced by the one
   rule in schema 151).
2. Home panel interval updates on the panel's own queue.
3. Schedules converted to tasks; the schedule engine, API and screens removed.
4. Event triggers as task wake-ups.
5. Regular checks as tasks.
6. Leftovers removed; docs and the full suites.

Built: step 1 (schema 145); step 2 (schema 146, `m20261124_000146_panel_intervals`):
panel intervals run on the panel's own update queue and panel schedules are paused;
step 3 (schema 147, `m20261124_000147_schedules_to_tasks`): every other schedule is a task,
and the schedule engine, API, proposals and screens are removed; step 4 (schema 148,
`m20261124_000148_trigger_tasks`): each event trigger has a task and a delivered event is a
run of it, with pause, resume and end kept together; step 5 (schema 149,
`m20261124_000149_check_tasks`): an assistant's regular check is a task with
`precheck='messages'`, edited from the Regular checks panel, and the check dispatcher is
removed; step 6: Activity › Calendar's List view and `GET …/activity/upcoming` are removed,
the agent instructions talk about tasks, and the docs are updated.

Schema 150 drops the old schedule tables and code (`m20261125_000150_drop_schedules`,
2026-11-25; the owner decided that while Cuckoo is in development it keeps no old
schedules, their data or code):
the schedule tables, columns, triggers and functions and the `schedule` task origin are
gone, and what still read them now reads tasks and their wake-ups (Activity's `task`
type, retry, the owner view, revocation, Library use where a run posts, closing a
workspace, and removing a member, which pauses their repeating tasks). See
[runtime/tasks.md](../../runtime/tasks.md#since-schema-150-the-old-schedules-are-gone).

Schema 151 is the one rule (`m20261125_000151_tasks_in_conversations`, 2026-11-25,
decision 1): every task and trigger runs in a conversation as its person; a task or
trigger whose only place was a work room moves to its person's direct chat with the
agent (opened when there is none; a task whose person left is paused with the
reason); `relay_tasks.room`, `relay_chats.work`, a trigger's `memory` and `room`,
`relay_event_deliveries.room`, `relay_subject_rooms` and `relay_run_posts` are
dropped, and `relay_event_triggers.chat` is required. See
[runtime/tasks.md](../../runtime/tasks.md#since-schema-151-one-rule-a-run-happens-in-its-conversation).

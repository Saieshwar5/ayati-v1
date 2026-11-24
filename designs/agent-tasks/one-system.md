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

1. **The channel rule (owner, 2026-11-24).** A task asked for in a channel or group
   belongs to that conversation: each run's result is posted there for everyone in
   it, but the agent works in a private work room with the person who asked
   (`relay_tasks.room`), so the channel sees one clean message per run and nothing
   when a run has nothing to report. Questions and approvals reach the person who
   asked through Needs you. A task asked for in a direct chat runs in that chat, as
   in schema 144.
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
   completed one a done task. It keeps running in its own work room and posts
   where it posted before (decision 1); one with no post target belongs to its work
   room and shows only in Activity. `relay_tasks.schedule` keeps the old id, so its
   earlier runs stay readable. The old row is marked moved and no longer runs.
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
   Calendar and task pages read tasks only (old occurrences only as history).

## Order

Each step is its own schema version and commit, with its tests passing before the
next starts.

1. Task rooms, instructions, minute rules and the channel rule.
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

# Tasks (schema 58; lifetimes, schema 144)

## Since schema 151: one rule, a run happens in its conversation

Built 2026-11-25 ([one system](../designs/agent-tasks/one-system.md), decision 1, the
owner's one rule). **A task run happens in the task's own conversation, as the person
it belongs to**, exactly like that person asking the agent there.

| What | Belongs to | Runs in | Access of |
| --- | --- | --- | --- |
| Task asked in a direct chat | the asker | that chat | the asker |
| Task asked in a channel or group | the asker | that channel or group | the asker |
| Event trigger | the person who set it up | the conversation chosen for it; none chosen: their direct chat with the agent | that person |
| Assistant's regular check | its person | their chat with the assistant, "Regular check" thread | its person |
| Weekly review | the person who turned it on | their direct chat with the agent | that person |

- **What a run sees.** It is an ordinary background request in that conversation
  (`chat_intake::background_in`, `relay_task_wakeups.reply`): its history is the
  conversation's recent messages, other people's included, and it reads the room's
  memory shelf in a channel or the person's shelf in a direct chat. A note it keeps
  wider than here still waits for a person (`background`). Event replies stay out of
  later history, as before (they carry event data).
- **What the conversation shows.** The run's input reads as one line, "↻ Following up
  on <task>" (an event run: "↻ <task>"), for every kind of wake-up (`follow_up.wakeup`
  says which), then the agent's reply, in the main timeline. A run that answers
  `NO_CHANGE` shows nothing. Nothing is copied anywhere.
- **Who it reaches.** Questions go to the run's person (Needs you); approvals to whoever
  decides them, as for any background request there.
- **Access, every run.** The ordinary intake checks the person may still ask the agent
  in that conversation; someone who left it is blocked ("Follow-up blocked: …") and
  the task needs fixing. A trigger's person must be able to ask the agent where it
  runs (`automation_access::may_ask_in`), which replaces "may post there".
- **Library.** A "viewers" document may be used in a run only if everyone in the run's
  conversation may see it (`relay_library_agent_use`).
- **Migration** `m20261125_000151_tasks_in_conversations`: a task in a real conversation
  stays there and loses `room`; a task or trigger whose only place was a work room (no
  post target, Activity only, a weekly review) moves to its person's direct chat with
  the agent, opened when there is none; a task whose person left the workspace is
  paused ("…no longer in the workspace, so it has no conversation to run in.") and a
  trigger whose person left is blocked. Triggers lose `memory` (and it leaves their
  saved input), `room` and the per-subject rooms; their `chat` is required. Work rooms
  keep their old messages but no people or agents (their queued requests are
  cancelled), and `relay_chats.work`, `relay_tasks.room`, `relay_event_deliveries.room`,
  `relay_subject_rooms` and `relay_run_posts` are dropped.
- **Removed:** work rooms (`work_rooms.rs`), posted run results and their guard,
  the trigger Memory setting, "Per subject" rooms and "Forget this subject", and the
  per-subject parallel runs: one trigger's deliveries run one at a time, in order.

Checks: PostgreSQL `tasks::conversation_tests` (a channel task's run is a job in the
channel as the asker with the channel's history and room shelf, its reply in the
channel and nothing copied, a question reaching only the asker; NO_CHANGE shows
nothing; someone who left the channel is blocked; a trigger runs in its conversation
or its person's direct chat; a trigger whose person left runs nothing) and
`tasks::conversation_migration_tests` (a populated schema 150 workspace upgraded); web
`agent-app-events`, `agent-events`, `agent-systems`, `activity` and `activity-tasks`.

## Since schema 150: the old schedules are gone

Built 2026-11-25. Migration `m20261125_000150_drop_schedules` drops the schedule tables
(`relay_agent_schedules`, `relay_schedule_occurrences`, `relay_schedule_changes`,
`relay_schedule_drafts`, `relay_home_schedule_bindings`), the schedule columns
(`relay_tasks.schedule`, `relay_review_routines.schedule`,
`relay_home_maintenance.schedule`), their triggers and functions, and the `schedule`
task origin (converted tasks are `origin='person'`, automatic ones `request`). Tasks
converted in schema 147 keep their rules and runs (linked through `relay_chat_jobs.task`)
and, since schema 151, run in their conversation; an old run reads as an ordinary request of its task and no longer
shows on the Calendar. A Home panel still linked to a paused or blocked schedule has its
update agreement paused for review.

What schedules answered now comes from tasks:

- **A task's run** is a request a task wake-up started (`relay_task_wakeups.reply`).
  Activity's type is `task` ("Task runs", `task_runs` in the totals); it can't be retried
  from Activity; the owner view refuses it; revoking access stops it.
- **Library use** in a task's run must suit everyone in the task's conversation, where
  it runs since schema 151 (`relay_library_agent_use`).
- **Removing a member** pauses their open repeating tasks (a rule or a booked follow-up;
  "Paused because the person it was for left the workspace.") and the handover summary
  counts them as repeating tasks paused for review.
- **Closing a deleted workspace** pauses its tasks with a booked run and cancels what is
  booked.

Checks: PostgreSQL `tasks::conversion_tests` (schedules from schema 146 converted through
147 and dropped at 150: nothing named schedule left, rules, rooms, old runs, and the next
run posting where it did), `member_handover_tests`, `settings::tests` (closing),
`library_tests::task_posts`; web `activity`, `people` and `settings` tests.

## Since schemas 148 and 149: triggers and regular checks are tasks

Steps 4 to 6 of [one system](../designs/agent-tasks/one-system.md), built 2026-11-24.
Everything an agent does on its own is now a task; a task wakes at a time (its rule),
when something happens (an event trigger) or to keep checking (a rule with a check in
code before the model).

- **Event triggers** (schema 148, `m20261124_000148_trigger_tasks`): each trigger has a
  task (`origin='event'`, `relay_event_triggers.task`) and each delivered event is a run of
  it, a started wake-up of kind `event`. Pausing or ending the task pauses or removes the
  trigger and the other way round; resuming happens on the agent page, where the trigger's
  access is reviewed again. See [shared agent events](shared-agent-events.md#since-schema-148-each-trigger-has-a-task).
- **Regular checks** (schema 149, `m20261124_000149_check_tasks`): an assistant's check is
  the task "Regular check" (`origin='check'`, `precheck='messages'`, a minute rule). A run
  looks in code first; with nothing new and no checklist it is recorded as no change with
  no model call. The assistant page's Regular checks panel edits the task. See
  [assistants](assistants.md#since-schema-149-the-regular-check-is-a-task).
- **Removed:** the regular-check dispatcher, Activity › Calendar's **List** view and
  `GET …/activity/upcoming` (old `tab=upcoming` links open the Calendar).
- **Agent instructions** talk about tasks: requests from schedules or events were tracked
  automatically before; now a task run is told which task it belongs to.

## Since schema 147: schedules are tasks

Built 2026-11-24, step 3 of [one system](../designs/agent-tasks/one-system.md) (decision
4). Migration `m20261124_000147_schedules_to_tasks` made every schedule that is not a Home
panel update (`relay_home_schedule_bindings`; schema 146 moved those to the panel's queue)
a commitment task with `origin='schedule'` (`person` since schema 150):

- **What it keeps.** Title = name; `instructions` = content; for = its person; the agent;
  `timezone` = its zone; `anchor_ms` = its first run; `schedule` = the old id (dropped in schema 150); `runs` = its
  number of runs; its created time.
- **Its rule**, from the interval: whole weeks → weekly on the first run's local weekday
  and time; whole days → daily at that time; whole hours that divide a day → hourly
  `from` the same minute (every 2 hours from 09:30 → `from` 01:30); anything else → a
  minute rule with the exact period (it may exceed 1,440 minutes; Cuckoo reads and shows
  it, "Every 36 hours", though an agent can still only give 15 to 1,440). One-time: no rule.
- **Where it ran** (the channel rule, replaced in schema 151): with a post target, `chat` =
  that conversation and `room` = its work room; without one, `chat` = its work room.
  Since schema 151 it runs in that conversation, or in its person's direct chat.
- **State.** Enabled: its next run booked at the old `next_ms` (`repeat`, or `follow_up`
  for a one-time one, whose brief now carries the instructions). Paused: a paused task
  ("Paused before it became a task."). Blocked: paused with the schedule's reason.
  Completed: done.
- **Earlier runs** are linked to the task (`relay_chat_jobs.task`; an automatic task left
  with no requests is deleted).
- **The schedule row** got `task` and was marked `state='completed'`; schema 150 dropped
  the schedule tables.
- **Weekly reviews** (Activity › Insights) are tasks: turning one on makes a weekly task
  "Weekly review" for that person (in their direct chat with the agent since schema
  151; `relay_review_routines.task`); off pauses it, on resumes it.

Removed: the schedule dispatcher, the schedule create/details/state/runs API (the
agent's conversations for posting moved to `GET …/agent-profiles/{a}/post-targets`),
schedule proposals in Chat and their cards, schedule items in Needs fixing, Coming up and
Planned, the schedule half of New automation (At a time now opens Chat) and the agent
page's schedule list. Checks: `getting_better` weekly review; the
conversion is now checked through schema 150 (above).

## Since schema 144: tasks with a lifetime

Built 2026-11-24 from [designs/agent-tasks](../designs/agent-tasks/README.md). The rule:
**people talk; the agent does; screens are only for looking and stopping.**

- **Made in chat, at once.** `create_task` takes `repeat` (a calendar rule), `first_at`
  (a local date and time), `until` (a local date) and `max_runs`. The task starts
  immediately, with no confirmation card; the agent says what it will do, when it runs
  next and what ends it. `change_task` changes the title, `done_when`, the rule, the
  first run, the end date or the run count, or pauses and resumes it. Agents may also
  create such a task on their own and say so. `propose_schedule` is retired; since schema
  147 drafts proposed before schema 144 no longer show.
- **The rule** (`relay_tasks.repeat`): `unit` hour, day, week or month; `every`; `at`
  (local `HH:MM`); `days` (ISO weekdays) for weeks; `day_of_month` (1–31, -1 for the
  last day; past a short month's end it runs on its last day) for months; `from`/`to`
  for hourly runs; `working_days`. The fields follow the iCalendar RRULE parts.
  `relay_task_next_due` works out the next run in PostgreSQL, in local clock time of the
  task's zone (the workspace's unless the task has its own), including daylight saving.
- **The workspace calendar.** `relay_workspaces.timezone` defaults to `Asia/Kolkata`
  (and empty ones were set to it); `working_days` defaults to Monday to Saturday;
  `holiday_rule` (`next`, `before`, `skip`) says what a working-days run due on a day off
  does; `relay_workspace_holidays` holds the days the workspace takes off. Cuckoo never
  guesses public holidays. Admins change them in Settings › General.
- **Cuckoo books every run.** When a run starts, the next one is booked from the rule in
  the same transaction (`relay_task_wakeups.kind='repeat'`, unique per task and due time,
  so two controllers can't start it twice). When the rule, `until` or `max_runs` leaves no
  next run, an `end` check closes the task once its last run has finished: done for a duty
  that ran out, failed ("Gave up after N runs: …") for a task with `done_when`.
- **Fixed behaviour, no settings.** A run found later than half its period (at least 10
  minutes, at most 12 hours) is skipped as missed and the next booked. A run due while the
  last one is still going is skipped. After three failed runs in a row Cuckoo pauses the
  task (`paused_reason` "Cuckoo paused it after 3 failed runs in a row.") and it shows in
  What needs fixing, as does a task that gave up in the last three days.
- **Each run starts fresh** in the task's conversation, for the person it is for, with a
  brief written by Cuckoo: "Task run N of M · title", the rule, what ends it, whether this
  is the last run and the newest notes. The agent leaves one line for the next run with
  `task_note` (`relay_task_notes`, 20 kept). A run that answers `NO_CHANGE` is recorded as
  "no change" (`quiet`) and its request and reply are kept out of the conversation.
- **Long jobs.** `continue_later {task, done, next}` saves where a part stopped and books
  the next part a minute later (`kind='continue'`); after 20 parts the agent must ask.
- **Limits.** 20 open repeating tasks per person per agent, 100 per agent. A run that
  started on its own (an event or a task run) may create a repeating task only with an
  end (`until`, `max_runs` or `done_when`).
- **The agent is told the time.** `open_tasks` returns `now` (local) and `timezone`.

People:

- **Chat**: under the reply, one line with the task, its next run (`next_text`), **View**
  and, for the person it is for, **Undo** (cancels it).
- **Activity › Calendar** (replaces Planned): a week of runs in the workspace's time zone,
  past (done, failed, no change, skipped, missed) and planned (every booked run and the
  rule's later runs; since schema 148 a trigger's runs as its task's); each trigger once and paused tasks
  beside it. The **List** view was removed in step 6; old `tab=upcoming` links open the
  Calendar.
- **Task details**: the rule as one sentence, next run, runs so far, each run with what came
  of it, the agent's notes, **Pause** or **Resume**, **Skip next run** and **End task**, for
  the person it is for, the agent's owners and admins. Changes are asked for in chat.
- **Agent page › On its own**: its open tasks that run on their own (each trigger's task and an assistant's regular check among them) and a link to the
  Calendar.

API: `GET …/activity/calendar?from&to&scope` (six weeks at most), `POST
…/activity/tasks/{id}/pause {paused}`, `POST …/activity/tasks/{id}/skip`, `GET/POST
…/settings/calendar {working_days, holiday_rule, holidays}`. Task details add
`lifetime`, `runs`, `notes` and `paused_reason`; task rows add `repeats` and `paused`.

Checks: PostgreSQL `tasks::lifetime_tests` (local calendar, days off and holidays, India
and a daylight-saving zone; booking every run; quiet runs; missed and overlapping runs;
three failures pause; giving up; change, pause, skip and bounds; the background rule;
continue later) and `task_calendar::tests` (calendar and who sees it, workspace calendar,
the line under the reply); guest `task_schema`; web `tests/agent-tasks.test.mjs`.

Not built: conditions Cuckoo checks itself in a connected app (the agent checks
`done_when` at each run), reading an action back to confirm "done", a spending limit per
task (spending isn't recorded per request while the model gateway is off), briefs and
alerts outside the app, a time zone per agent, and runs with a real model. (Schedules
moved into tasks in schema 147, triggers in 148 and regular checks in 149.)

The sections below describe the original increment.


People-and-agents-first, Phase 2 (planned in `proj-docs/people-and-agents-first/tasks.md`).
**A task is a promise with an outcome.** It groups the requests that work towards one
result, so people see what agents have taken on, and an agent never forgets a promise.

## Two kinds

| Kind | Opened by | Closes |
| --- | --- | --- |
| **automatic** | The platform, when a request changes a real system (a write action that isn't blocked), asks a question, hands work to a helper, or comes from an event | With that request: complete is done, cancelled is cancelled, anything else failed |
| **commitment** | The agent's `create_task`, or a person's **Make this a task** | Only when the agent calls `close_task` or a person closes it |

A Home panel refresh is a panel update, never a task. Answering, explaining, drafting text
for the person or giving an opinion is conversation: no task.

## Status

`relay_task_status(workspace, task)` works the status out from the task's requests; only
the closing outcome is stored.

| Status | Means |
| --- | --- |
| working | A request is queued or running |
| waiting | A request waits for an approval, an answer, a helper or a time (`waiting_on` names which) |
| needs_fixing | A commitment whose latest request could not complete, or whose follow-up couldn't run |
| late | A commitment past its due time |
| scheduled | A commitment that starts later, or has a follow-up booked |
| open | A commitment with nothing running, not yet due |
| done / failed / cancelled / not_task | Closed |

## Agent tools (ayati_control)

- `open_tasks {}`: this agent's open commitments for this person or conversation.
- `create_task {title, done_when?, start_in_minutes?, due_in_minutes?}`: title up to 160
  characters, done_when up to 400. `start_in_minutes` (1 minute to 30 days) books a
  follow-up for then; `due_in_minutes` (up to a year) sets when it's late. If the request
  already has an automatic task, that task becomes the commitment (one task, not two). A
  different promise in the same request is refused.
- `follow_up {task, in_minutes, note?}`: check this open commitment again later (1 minute
  to 30 days; note up to 400). One follow-up per task: a new one replaces the old.
- `attach_to_task {task}`: this request continues an open task of this agent, for the same
  person or conversation. An automatic task the request had is dropped.
- `close_task {task, outcome: done|failed|cancelled, result?}`: result up to 1000 characters.
  Repeating the same outcome is harmless.

The agent's instructions carry the promise rule: never say "I'll…" without a task; check
`open_tasks` and attach instead of promising twice; to check, chase or come back later,
book a follow-up. A panel update task can't use these tools.

## Follow-ups (agents wake themselves)

When a follow-up is due, the scheduler loop (pooled executors only)
starts a request for the agent **in the task's own conversation, for the person it
promised**, already attached to the task. The request says "Follow-up · <title>" with
what the task is for and the agent's note; Chat shows it as one light line, "Following up
on <title>", not as if the person had asked again.

- Ordinary intake applies: the person must still be able to ask that agent there. If not
  (they left the conversation, lost access, the agent was removed) the follow-up is
  **blocked**, the task **needs fixing**, and its details say why.
- If work on the task is still running, or agents are paused or the queue is full, the
  follow-up waits five minutes and tries again.
- Closing a task (agent or person) cancels its follow-up.

Task rows show the due time and the next check; details show "Next check" and, for a
blocked one, "The agent couldn't continue: …".

## People

- **Activity → Work → Tasks**: every open task the person may see, needs fixing first, then late,
  waiting, working, open and scheduled. My agents narrows it. At most 200 are listed.
- **Task details**: what it promises (done when), who it's for, where, its steps (each
  request, opening its own details) and its result. **Mark done** and **Cancel task** are
  for the person it's for, the agent's owners and admins; **Not a task** is for anyone
  who can see it. Each close is in the audit log.
- **Make this a task** on a request's details turns it into a commitment for the person who
  asked, kept open until someone closes it.
- **Chat** shows a chip under an agent reply: "Task: Send quotes to Acme · Open →" for a
  promise; work the platform tracked shows ("Tracked: …") only once it waits, needs a look
  or is done. History shows a request's task, and conversation turns that aren't part of
  any task read lighter.

**Who sees a task:** the person it is for, anyone who can see its conversation, the agent's
owner and admins.

## API

Under `/api/identity/workspaces/{w}/activity`:

- `GET /tasks?scope=mine|all` — open tasks.
- `GET /tasks/{id}` — one task with its requests and `may_close`.
- `POST /tasks/{id}/close {outcome: done|cancelled|not_task, result?}`.
- `POST /requests/{reply}/task {title?}` — Make this a task.

History items and request details carry `task {id, title, kind, status}`; Chat messages
carry the same `task` for agent replies.

## Storage

`relay_tasks` (kind, origin, title, done_when, requester, chat, start/due times, outcome,
result, who closed it), `relay_task_wakeups` (task, due time, note, pending | started |
cancelled | blocked, the request and message it started, why it was blocked) and
`relay_chat_jobs.task`. Upgrade tests run current code on older
schemas, where requests are simply not tracked (`tasks::available`).

## Checks

- PostgreSQL `tasks::tests`: a write makes a task that closes with its request and shows in
  Chat; an agent's promise stays open, takes more work and closes; a question waits and a
  failed promise needs fixing (listed first); people make and close tasks and only those
  who may see one do.
- PostgreSQL `tasks::wakeups::tests`: a follow-up is scheduled, wakes the agent once for
  the same person in the same conversation, waits while work runs, and closing cancels
  it; one that can't reach the person blocks and the task needs fixing; past due is late.
- Guest `task_schema`: the tools forward only their bounded fields.
- Web `tests/activity-tasks.test.mjs`.
- Journey `tests/agent_tasks.mjs` (real Goose, scripted model): promise, attach, close;
  and a booked follow-up woken by the scheduler that closes the task.

# Agent tasks and calendar: implementation plan

Approved 2026-11-24. Built on the existing task system (`relay_tasks`, schema 58)
and its follow-ups (`relay_task_wakeups`), so a task made in chat, its runs, its
status and its place in Activity stay one thing. Briefs (step 6) and alerts
outside the app (step 8) are not part of this build.

## Why build on tasks, not schedules

A commitment task already belongs to a person, a conversation and an agent, runs
its follow-ups in that conversation, and is closed by the agent or a person. A
schedule is fixed once saved, runs as the person who saved it in a hidden work
room, and also drives personal Home panel updates. Extending tasks gives "say it
in chat, the agent does it" directly. Existing schedules and event triggers keep
their own engine and are shown in the same lists (step 9); nothing that runs today
stops.

## Schema 144: task lifetimes

`m20261124_000144_task_lifetimes`, forward only.

- `relay_tasks` gains:
  - `repeat JSONB`: the calendar rule (below); null for a task that does not repeat.
  - `timezone TEXT`: null means the workspace's.
  - `until_ms BIGINT`: ends on this date; `max_runs INT` (1–1000): ends or gives up
    after this many runs; `runs INT`: runs started so far.
  - `paused_ms BIGINT`, `paused_reason TEXT`: paused by a person or by Cuckoo.
  - `pieces INT`: parts of a long job continued later.
- `relay_task_wakeups` gains `kind` (`follow_up`, `repeat`, `end`, `continue`),
  the state `skipped`, and a unique index on `(workspace, task, due_ms)` for
  repeats, so two controllers can never book or start the same run twice.
- `relay_task_notes`: one line per run that the next run reads (20 kept per task).
- `relay_workspaces` gains `working_days` (default Monday to Saturday) and
  `holiday_rule` (`next`, `before`, `skip`); `timezone` defaults to
  `Asia/Kolkata` and empty ones are set to it.
- `relay_workspace_holidays (workspace, day, name)`.
- SQL function `relay_task_next_due(rule, timezone, anchor_ms, after_ms, workspace)`
  returns the next run after a moment, in local clock time, skipping or moving days
  off. PostgreSQL does the time-zone and daylight-saving work, so no new
  dependency is needed.
- `relay_task_status` gains `paused`.

### The calendar rule

| Field | Meaning |
| --- | --- |
| `unit` | `hour`, `day`, `week` or `month` |
| `every` | 1–365 units between runs |
| `at` | local time `HH:MM` (days, weeks, months; default 09:00) |
| `days` | ISO weekdays 1–7 for weeks (default: the first run's weekday) |
| `day_of_month` | 1–31, or -1 for the last day (months) |
| `from`, `to` | local `HH:MM` window for hourly runs |
| `working_days` | only on working days; holidays follow the workspace rule |

"Every weekday at 9" is `{unit: week, days: [1,2,3,4,5], at: "09:00"}`. "Every 2
days at 10:00" is `{unit: day, every: 2, at: "10:00"}`. The fields follow the
iCalendar RRULE parts (FREQ, INTERVAL, BYDAY, BYMONTHDAY, BYHOUR); a full RRULE
parser is not needed for what people ask in chat.

## Step 1: task engine

- After each run starts, Cuckoo books the next run from the rule in the same
  transaction. The agent never has to book it.
- When the rule, end date or run count says there is no next run, Cuckoo books an
  end check. Once the last run has finished, the task closes: done for a task that
  simply ran out, failed ("gave up after N runs") for a task that was waiting for
  something to become true.
- Missed runs: a run found later than half its period (at least 10 minutes, at most
  12 hours) is skipped and recorded, and the next one is booked.
- Overlap: a run due while the task's last run is still going is skipped and
  recorded.
- Failures: after three failed runs in a row the task is paused by Cuckoo and shows
  in Needs you.
- Each run starts fresh with a short brief written by Cuckoo: the task, run N of M,
  the rule, what ends it, and the last notes.

## Step 2: tasks in chat

- `create_task` gains `repeat`, `first_at` (local date and time), `until` (local
  date) and `max_runs`. The task starts at once; no confirmation.
- `change_task`: title, what ends it, the rule, the end date, the run count, pause
  and resume. Cuckoo books the next run again.
- `task_note`: one line for the next run.
- `propose_schedule` is retired from the agent's tools; the agent creates the task.
- A run started by a schedule, event or task run may create repeating tasks only
  with an end (a date, a count or what ends it).
- Limits: 20 repeating tasks per person per agent, 100 per agent.
- The task line under the reply shows the next run and Undo, for the person it is
  for, the agent's owners and admins. Undo cancels the task.

## Step 3: safe running

- A task run that finds nothing to report answers `NO_CHANGE`. Cuckoo records the
  run as "no change" and keeps it out of the conversation.
- Pause, resume and skip next for people (the person it is for, owners, admins),
  audited.

## Step 4: screens

- Agent page, "On its own": the agent's open tasks with a rule or a next run, then
  its schedules and triggers.
- Activity › Planned becomes Calendar: a week of runs, past (done, failed,
  skipped, no change) and planned, from task rules, follow-ups and schedules;
  tasks with no fixed time and paused tasks in a side list.
- Task details: the rule as a sentence, next run, notes, Pause or Resume, Skip next,
  End task.
- Needs you: paused tasks and tasks that gave up.

## Step 5: settings

Settings › General: time zone (India by default), working days, holidays and what
happens to a run due on a day off.

## Step 7: tasks that end by themselves

- `done_when` is checked by the agent at every run; reaching it means `close_task`.
- The give-up point is `max_runs` or `until`; reaching it closes the task as gave up.
- Not in this build: conditions Cuckoo checks itself in a connected app, and reading
  an action back to confirm "done". Both need per-app work.

## Step 9: events and checks in the same lists

Event triggers and an assistant's regular checks appear in the agent's list and the
calendar side list. Their engines are unchanged. Moving their rows into tasks is
later work; it also needs a new home for personal Home panel updates, which use
schedules today.

## Step 10: long jobs

`continue_later {task, done, next}`: the run saves what is done and what is next as a
note, and Cuckoo starts the next part in a minute. At most 20 parts per task; then the
agent must ask the person. The 180-second run limit stays until real-model tests
show what is needed.

## Checks

- PostgreSQL tests: next-run calculation (daily, weekdays, every N days, monthly and
  last day, hourly window, India and a daylight-saving zone, days off and
  holidays); booking once; missed and overlapping runs; three failures pause;
  end and give-up; change, pause, resume, skip; notes; limits; continue later.
- Guest schema tests for the new tool fields.
- Web tests for the task line, task details, calendar and settings.
- `cargo fmt`, `cargo clippy`, the web build and `./scripts/check-file-lines.sh`
  where present.

## Not in this build

Briefs, alerts outside the app, conditions checked in code, read-back checks,
moving schedules and triggers into tasks, a per-task spending limit (spending is
not recorded per request while the model gateway is off), and runs with a real
model.

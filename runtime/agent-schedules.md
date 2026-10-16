# Per-agent schedule management

Open Agents → Automation → Schedules (or Details → Manage schedules). The work panel belongs to the selected
agent and names that agent. It supports one-time work and fixed intervals using
the existing host-side durable scheduler. No new service or dependency is added.

Create a task instruction and a future first-run time in the browser's displayed
timezone. A repeating schedule uses elapsed minutes, hours or 24-hour periods;
it is not calendar recurrence and can shift local clock time across daylight
saving changes. Invalid/nonexistent local times are rejected. Ambiguous repeated
clock times use the browser's Date interpretation. The UTC instant is saved and
subsequently displayed in the current browser timezone. API timing semantics are
unchanged; see [scheduled intake](scheduled-work.md).

Scheduled work can call the configured provider and incur API charges without
an open web page. The host and Ayati daemon must stay on. Tools retain existing
approval checks and the agent uses its current role/model/tool configuration.

The list shows original task instructions, first run, state and next due time.
Pausing stops queued occurrences and future dispatch of that routine, leaving
active work running; use the existing task Stop control for active work. Resuming
retains the timing anchor, coalescing missed intervals into the latest occurrence.
Stopping an agent VM persists dispatch pause independently of each schedule's
state. The dialog reports this; a new message or explicit VM Start can resume
dispatch. Neither schedule creation nor schedule resume clears dispatch pause.

A completed one-time schedule means its occurrence was accepted, not that the
model succeeded. View runs to inspect the most recent 20 occurrences and their
current task status. Open a run to read its saved request and replies, including
older results outside the recent conversation window. Use Refresh to update
expanded run history. Unknown or foreign-agent IDs are rejected by the selected
agent's private store.

`GET /api/schedules` retains its `schedules` array and adds `available` (Goose
runtime) and `dispatch_paused`. `GET /api/schedules/{id}/runs` returns
`{"runs":[{"due_at_ms":123,"task":{...}}]}` newest first, bounded to 20.
The existing selected-agent header and loopback/same-origin protections apply.
These remain local-owner APIs; staff permissions and hosted identity are later
work. No schema migration is required.

Creating a schedule keeps its UUID and exact payload until acknowledged. The
browser tab's session storage preserves an unacknowledged attempt across reload
when available. Retry same save sends that same payload; the server's existing
idempotency prevents duplicate schedules. No automatic POST retry is introduced.
If storage is unavailable, this protection survives only while the page stays
open. Failed list refresh disables creation and list actions until reconnection.
The form draft is not reset by routine list refreshes.

Schedules retain immutable instructions/timing in this increment. To change them,
pause the old routine and create another. Editing/removal, calendar/weekday/cron
recurrence, named timezone selection, trigger rules and connector subscriptions
remain later increments. There are at most 100 stored schedules per agent under
the existing proof limit. Home panels and Apps are separate future work.

Checks: `cargo test --test schedule_history --test scheduled_intake`,
`node --test tests/agent_schedules.mjs tests/workspace_ui.mjs`, and the selected
owner-creation scenario in `tests/independent_agents.mjs` with a disposable VM
manifest. Tests use a synthetic provider and temporary data; no paid inference.

Schedule controls now use the shared [Agents work panel](agents-workspace.md):
beside chat on desktop and full-screen on narrow displays. The Triggers tab is
an explicit unavailable state, not a working event-rule editor.

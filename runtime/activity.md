# Activity (schema 48)

One page for everything agents do (D235, built in M4). It sits in the main menu
between Chat and Agents (`/w/<workspace-id>/?view=activity`) and reads existing
records: chat requests, tool actions, approvals, questions, schedule and trigger
runs, hand-offs and agent setting changes. Opening it never starts work.

## Views

- **Needs me** (default; `&tab=needs`): approvals, questions and work that needs
  a look, from every agent, grouped. The tab shows the count. Opening an item
  saves a personal read receipt first; if that fails the error shows and nothing
  opens. Reading never answers or approves. **Mark reviewed** clears a failed or
  interrupted outcome without retrying or changing it. It replaces Home's former
  "Your attention" pop-up; Home keeps a small panel that links here.
- **Now** (`&tab=now`): "While you were away", one card per agent (Working,
  Waiting, Failing, Paused or Idle with its next schedule run) and requests
  running now with **Stop**. It follows the live change stream and polls every
  10 seconds as a fallback.
- **Tasks** (`&tab=open`): every open task the person may see (needs fixing first), with
  each task's details, steps and closing. See [tasks](tasks.md). `&task=<id>` opens one.
- **History** (`&tab=history`): filters in one row (period, agent, person, where,
  type, status) plus search; totals for the period; two charts (M7): requests per
  day (empty days included, the busiest labelled, a tooltip on hover or focus) and
  requests by agent (top 12), each with **Show as table**; a timeline grouped by
  day with **Show older**. **Export CSV** downloads the filtered requests (UTC
  times, at most 5,000; cells that look like formulas are escaped). Type "Setting
  changes" lists agent profile and owner changes.
- **Request details** (a drawer from any row): the asking message, each tool step
  with how it was allowed (automatically, approved by whom, blocked, unconfirmed)
  and its fields, the reply, **How to fix it** for failures, and a warning when
  the reply claims a change (sent, updated, …) that the records don't show.
  Running work can be stopped here.

An agent's **Activity** tab is History filtered to that agent, with its own
search. Direct chats with an agent say "Activity with <agent> is visible to its
managers and admins." In Chat, an agent reply's ⋯ menu has **See what it did**,
which opens the same request details.

## Who sees what

A request is visible to the person who asked, anyone who can see its conversation,
the agent's managers, and admins. Work that used Library sources stays with its
requester. Losing access to a channel hides its requests again. When a manager or
admin opens a request from a conversation they are not in, the details say so and
the view is recorded in the audit log (`activity_private_viewed`).

## Panel updates

A Home panel refresh is a request of its own type, `panel` (it is linked in
`relay_home_update_runs`), even when a routine starts it. It reads "Updated panel:
<title>", names its panel and is filtered with Type → Panel updates. Only the panel's
owner, the agent's managers and admins see it; being in the panel's source
conversation is not enough. History folds routine updates into one line per panel per
day ("Updated panel: Sales today · updated 12 times"), which expands to each update;
an update that could not complete keeps its own line.

## Events not handled

An event that arrived but no agent acted on (its trigger was paused or blocked, all
agents were paused, or access changed) never becomes a request, so it has its own
list: History → Type → **Events not handled**, newest first by arrival, with the
recorded reason ("All agents were paused when this event arrived, so it was not
handled."). Now shows "N events not handled today" with **See why**, which opens that
list. Only the person who set up the trigger, the agent's managers and admins see an
event, and only its trigger's name, agent, time and reason: never the event's
contents. My agents narrows it to triggers the person set up or agents they manage.

## My agents

`scope=mine` narrows History, totals, export and Now to the requests the person
made and the work of agents they manage (`relay_agent_managers`); on Now, only the
agents they manage get a card. `scope=all`, or no scope, keeps everything the
visibility rule above allows. The scope only narrows; it never shows anything the
person could not see. An admin is a manager of the agents they own, not of every
agent, so an admin's Mine is their own agents and requests. The owner chose My
agents as the screen's default for everyone (2026-11-01); the switch itself waits
for an approved design.

## Controls

Activity is where people control agents as well as watch them
(people-and-agents-first, Phase 1b). Every control uses a server route with its own
checks; the screen only hides what a person may not do.

- **My agents / All agents** on Now and History (not on Needs me, whose items are
  already the person's own decisions and questions). The choice is remembered on the
  device, per person and workspace; the default is My agents.
- **Pause or resume one agent** from its card: `POST
  /api/identity/workspaces/{w}/agent-profiles/{id}/pause` with `{"paused": bool}`.
  The agent's owner, its managers and admins may do it (D231). It has the same
  effect as the Agents page's pause and writes the same readable audit sentence
  ("Meera paused Atlas"); asking for the state it already has changes nothing. The
  card shows the button only when `can_pause` is true. While all agents are paused
  the card has no button; the workspace switch decides.
- **How to fix it** on a failing card opens today's latest failed request
  (`last_failed`), whose details explain the fix.
- **Retry** in a failed request's details: `POST …/activity/requests/{reply}/retry`.
  Only the person who asked; only a request that could not complete, that a person
  started with their own message and that had no files. It sends the same message
  again in the same conversation and thread, quoting the failed reply, so the agent
  and Chat see the link; every ordinary intake check applies. A request with an
  action whose outcome is unknown is refused: the action may already have happened
  (D228). The message id is derived from the request and the person, so a double
  click or two tabs give one retry.
- **Discuss in chat** in any request's details opens the person's direct chat with
  the agent (creating it when needed) and starts the message with what it is about,
  without replacing an existing draft.

## Supervising at scale (schema 61)

People-and-agents-first, Phase 4. For the people who can fix an agent (its owner,
managers and admins; with My agents, the agents they manage):

- **Needs fixing** on Now, grouped by cause, each with one fix:
  - an account needs reconnecting (every agent using it; **Reconnect** in Settings for
    admins, "Ask an admin to reconnect" for others);
  - actions whose outcome couldn't be confirmed (Check it);
  - agents failing twice or more today (How to fix it, the latest failure);
  - routines or event triggers that stopped (Review routines/triggers on the agent);
  - requests waiting on a person for over 4 hours (Open Needs me);
  - requests running far longer than the agent usually takes (3 × its median of the
    last 30, at least 10 minutes);
  - what agents reported they need (see below; Dismiss);
  - tasks that need fixing (Open Tasks).
  An agent in any group shows **Needs fixing** on its card. Someone who only talks to
  an agent isn't asked to fix it.
- **Agents report needs in a fixed shape.** The `report_problem {cause, detail, fix?}`
  tool (cause: connection, permission, information, external or other; detail up to 300
  characters; fix up to 200; at most five per request) saves it for the people who can
  fix the agent (`relay_agent_needs`), shown for 24 hours or until dismissed. The agent's
  instructions say to use it instead of a raw error, then tell the person and stop.
- **Coming up**: the next 24 hours of routines (Pause), follow-ups (Cancel follow-up;
  the task stays open) and tasks due, soonest first. Routines bound to a Home panel are
  left out. Follow-ups and due tasks show to the person the task is for and the agent's
  fixers; routines to its fixers and the routine's person.
- **Needs me at volume**: approvals carry their operation and the backup clock. Items
  where an admin may already step in come first, then the longest waiting; approvals
  for the same action from the same agent fold into one row ("3 × Refund order") with
  **Review together**: one screen listing each request's details, each approved or
  rejected on its own. **Approve all N** sends the decisions one by one; the server
  still checks every one.

## Steering and routines from Chat (schema 62)

- **Add to a running request** (Phase 4 AC6): the person who asked adds a note from the
  request's details ("Add to this request") or the task's ("Add to this task", which
  goes to the task's running request). It reaches the agent with the result of its next
  tool call as `person_added` (once), instead of waiting behind it as a new request; the
  agent's instructions say to take it into account. Anything the agent never read
  becomes a new request from the same person when it finishes ("Added while you were
  working on my last request: …"); a stopped request carries nothing. Request details
  list what was added and whether the agent read it. At most 20 per request, 2,000
  characters each; never on a finished request.
- **Routines from Chat** (T3): for something that repeats, the agent calls
  `propose_schedule {name, instructions, first_in_minutes, every_minutes?}` (every hour to
  every 30 days, or once; at most three per request). It shows under the agent's reply
  ("Routine: Monday sales summary · Every week, first Mon 9:00") and is saved only when
  the person who asked presses **Save routine**, through the ordinary routine checks;
  "Not now" dismisses it. A retried save is the same routine. "Remind me on Friday" is a
  task with a start time (`create_task start_in_minutes`, Phase 2).

## Review: getting better over time (schema 63)

People-and-agents-first, Phase 5. Activity → **Progress** (`&tab=review`), for the people
who can fix an agent (owner, managers, admins; My agents narrows it):

- **Measures, last 30 days**, per agent and per person: tasks done of all, failed, done
  on time of those with a due time, tasks that needed a person, **person-minutes** (from
  reading a question or approval to answering it, at most ten minutes each; read times
  come from Needs me receipts), requests and model runs. Counts, not money: there are
  no per-call token counts yet (D232).
- **Let agents do more**: five or more approvals of the same action for the same agent
  in 30 days, and nothing else decided for it, offers "Let it" — the agent's own rule
  becomes Allow through the ordinary rule path, so it is never looser than the admin's
  rule (nothing is offered while the admin's rule says Ask) and it can be changed back
  on the agent's Tools tab. "Keep asking" hides the offer for that person.
- **Goals**: up to five per agent (name, target, unit, per week or month), set here. The
  agent reads them (`my_goals`) and reports progress and an early risk (`update_goal
  {goal, current, risk?}`); Review shows a progress bar and the risk.
- **Weekly review**: "Turn on" makes a weekly routine for the agent (run as the person
  who turned it on, in its own work room) in which it writes `write_review {done,
  went_wrong, learned, suggestion?}`, once per request. Review lists the last 30 days'
  reviews with any suggested change and a link to set up the agent.

Home (HP7): a one-line "N things need you →" when anything does; "You asked Nova “How
many orders today?” 4 times lately — Ask for a panel" from the person's own repeated
questions in the last 14 days (at most three, "Not now" per device); and "2 panels
haven't changed in two weeks · Hide them".

## "While you were away"

The summary covers everything since the person last caught up, stored per person
on the server (`relay_activity_visits`), so it agrees across devices. Before the
first catch-up it covers the last day. Only **See in History** or **Dismiss**
records a new catch-up; opening the page does not.

## API

- `GET /activity/now` also returns `fixing` (groups) and `coming_up`, and each agent's
  `needs_fixing` count. `POST /activity/needs/{id}/dismiss`;
  `POST /activity/tasks/{id}/follow-up/cancel`; `POST /activity/requests/{reply}/steer {text}`,
  `POST /activity/tasks/{id}/steer {text}`, `POST /activity/routine-proposals/{id} {save}`.
  Request details add `added`; Chat agent replies add `routines`.
- `GET /activity/review?days&scope`; `POST /activity/rule-suggestions {agent, connection,
  operation, accept}`, `/activity/goals {agent, id?, name, target, unit, period}`,
  `/activity/goals/{id}/archive`, `/activity/weekly-review {agent, enabled}`;
  `GET /home-panels/suggestions`. Attention approvals add `operation`,
  `connection`, `operation_title`, `created_at` and `backup_at`.

All under `/api/identity/workspaces/{w}/activity`, authenticated by the session
cookie; every query applies the visibility rule above on the server.

- `GET /history?agent&person&chat&status&kind&from&to&q&before&scope` — 25 requests,
  newest first; `next_before` is the cursor (no gaps or repeats).
  `status` is one of active, waiting, done, failed, review, stopped; `kind` one of
  chat, schedule, trigger, handoff; `scope` one of mine, all (default all).
- `GET /totals?…&offset` — counts and per-day/per-agent numbers for the same
  filters; `offset` is the browser's minutes east of UTC.
- `GET /export?…` — the same filters as a CSV file; the `x-cuckoo-truncated`
  header says whether more than 5,000 matched.
- `GET /changes?agent&before` — agent setting changes as sentences.
- `GET /now?today=<ms>&scope` — agent cards, running requests and the away summary.
- `POST /seen` with `{}` — record a catch-up.
- `GET /requests/{reply}` — one request's details.
- `POST /requests/{reply}/retry` — ask a failed request again (see Controls).
- `GET /unhandled?from&before&scope&agent` — 25 events not handled, newest first;
  `next_before` is the cursor. Now's `unhandled_today` counts today's.

## Storage and performance

Schema 48 adds `relay_activity_visits` and indexes on requests by workspace
(newest first, and by time), agent, requester and action job. Requests from before
`telemetry.sql` get their reply's time as `created_ms`, which becomes NOT NULL, so
every filter uses one indexed column; queue metrics still skip them because they
have no start or finish time. The asking message comes from
`relay_chat_jobs.request_message` (older requests find the message just before the
reply) and is looked up only for rows that are shown or searched. Activity reads
run with JIT off: their estimates can exceed `jit_above_cost`, and compiling then
cost about a second.

Measured locally with 30 agents and 81,000 requests over 90 days
(`thirty_agents_over_ninety_days_stay_within_the_response_budget`): history
170 ms (7 days) / 440 ms (90 days), totals 50 / 620 ms, search 580 ms, Now 160 ms.

## Checks

- PostgreSQL: `activity_panel_tests` (panel type, title and owner-only visibility),
  `activity_unhandled_tests` (reason, visibility, no contents, Now's count);
  `activity_controls_tests` (pause by owner, manager and admin but not
  others; retry only by the asker, once, never after an unconfirmed action);
  `activity_scope_tests` (My agents for a manager, an admin and an
  unknown scope across History, totals, export and Now); `activity_tests` (visibility for requester, conversation members,
  managers, admins and after leaving a channel; private-view audit; paging without
  gaps; filters and totals; older requests' summaries; away summary across
  catch-ups; disagreement warning; the 81,000-request budget).
- Web: `tests/activity.test.mjs`, `tests/activity-panels.test.mjs`, `tests/activity-unhandled.test.mjs`, `tests/activity-controls.test.mjs` (scope switch,
  pause with a double click, How to fix it, Retry only for the asker, Discuss in chat),
  `tests/workspace-attention.test.mjs`.
- Browser journeys with real Goose: `workspace_attention`, `channel_approvals`,
  `agents_team_workspace`.

Chart colour: one series each, `#2a78d6` in light mode and `#3987e5` in dark,
both checked against the surfaces with the data-visualisation palette validator.

**Waiting for a design:** Home's Working now and Today's totals panels (M7). Home
keeps its Notifications panel, which opens Needs me.

Supervising at scale: PostgreSQL `activity::now::fixing` (each cause grouped with its fix
for fixers only, report_problem refused for made-up causes, dismiss by fixers only;
Coming up soonest first), web `activity-supervising` (Needs fixing, Coming up's Pause
and Cancel follow-up, Review together and urgency order), guest `task_schema`
(report_problem), journey `tests/agent_needs.mjs` (real Goose reports a need).
Steering and routines: PostgreSQL `activity::now::steering` (an addition reaches the next
step once and is carried into a new request if unread; a task's addition reaches its
running request; a proposed routine is saved only by the person who asked, once), web
`activity-steering`, journey `tests/agent_steering.mjs` (real Goose reads the addition
with a tool result; its weekly routine is saved on confirmation).
Review: PostgreSQL `activity::now::better` (rule offered only under an Allow ceiling and
taken through the rule path; goals, reviews once per request, measures, weekly routine
on and off, fixers only; panel suggestions from the asker's own messages), web
`activity-review`, journey `tests/agent_needs.mjs` (real Goose reads its goal, reports
progress and risk, writes a review).

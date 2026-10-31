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

## "While you were away"

The summary covers everything since the person last caught up, stored per person
on the server (`relay_activity_visits`), so it agrees across devices. Before the
first catch-up it covers the last day. Only **See in History** or **Dismiss**
records a new catch-up; opening the page does not.

## API

All under `/api/identity/workspaces/{w}/activity`, authenticated by the session
cookie; every query applies the visibility rule above on the server.

- `GET /history?agent&person&chat&status&kind&from&to&q&before` — 25 requests,
  newest first; `next_before` is the cursor (no gaps or repeats).
  `status` is one of active, waiting, done, failed, review, stopped; `kind` one of
  chat, schedule, trigger, handoff.
- `GET /totals?…&offset` — counts and per-day/per-agent numbers for the same
  filters; `offset` is the browser's minutes east of UTC.
- `GET /export?…` — the same filters as a CSV file; the `x-cuckoo-truncated`
  header says whether more than 5,000 matched.
- `GET /changes?agent&before` — agent setting changes as sentences.
- `GET /now?today=<ms>` — agent cards, running requests and the away summary.
- `POST /seen` with `{}` — record a catch-up.
- `GET /requests/{reply}` — one request's details.

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

- PostgreSQL: `activity_tests` (visibility for requester, conversation members,
  managers, admins and after leaving a channel; private-view audit; paging without
  gaps; filters and totals; older requests' summaries; away summary across
  catch-ups; disagreement warning; the 81,000-request budget).
- Web: `tests/activity.test.mjs`, `tests/workspace-attention.test.mjs`.
- Browser journeys with real Goose: `workspace_attention`, `channel_approvals`,
  `agents_team_workspace`.

Chart colour: one series each, `#2a78d6` in light mode and `#3987e5` in dark,
both checked against the surfaces with the data-visualisation palette validator.

**Waiting for a design:** Home's Working now and Today's totals panels (M7). Home
keeps its Notifications panel, which opens Needs me.

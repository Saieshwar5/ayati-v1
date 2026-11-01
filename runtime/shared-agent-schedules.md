# Shared agent schedules

## Since schema 56: runs happen in Activity, not in Chat

Built 2026-11-01 (D237, milestone B1). The rule: Chat is for conversations
people start or join; work an agent does on its own appears in Activity.

- **Work rooms.** Each schedule runs in its own private room
  (`relay_chats.work`), holding the person who set it up and the agent. It is
  opened at the first run and never appears in Chat, its directory or alerts.
  People can't write into it. It is also the schedule's memory: each run reads
  the earlier runs, never anyone's conversation.
- **Where results go.** A schedule's conversation (`chat`) is now optional and
  means "also post the result to". The default is **Activity only**. When a
  conversation is set, a completed run posts one short message there: the
  schedule's name, the first 400 characters of the result and a
  `[View run](run:<id>)` link that opens the run in Activity. The outcome is
  recorded in `relay_run_posts` (posted, or skipped with a reason).
- **Existing schedules** keep their conversation as the post-to target, so
  nothing quietly disappears.
- **Authority.** A run needs its person's current maintainership of and direct
  access to the agent. A schedule that posts somewhere also needs the
  conversation authority it always did (membership, agent participation,
  posting, an open channel). Losing that blocks future runs for review, as
  before. A run already accepted continues in its room, and its post is
  skipped with a reason.
- **Activity.** A run's details show **Started by** (the schedule), **Set up
  by** and **Results** (posted or not, and why), with **Pause schedule**.
  There is no conversation to open. A run's question and approvals wait in
  **Needs me**. Links of the form `?view=activity&tab=history&run=<id>` open a
  run directly. In an agent's Schedules, **View runs** opens each run in
  Activity, and the create dialog asks "Where results go".
- **Unchanged:** personal Home panel updates keep their own source.
  Event-triggered work still posts in its channel until milestone B1b.

The sections below describe the original schedules increment. Where they say
runs enter the source conversation, the work room above replaces that.

Implemented locally 26 October 2026, serially after PR 72. The initial plan was
saved before source/UI edits. This increment adds shared scheduled intake; external
events and channel handoffs remain separate.

The existing private-runtime scheduler remains intact. Managed-agent one-time
and fixed-interval schedules now live in PostgreSQL under Agents → Automation.
Each immutable instruction records its original human authorizing principal,
target agent and existing source conversation. Creating or resuming a schedule
requires current agent maintainership, request permission, source membership,
agent participation and an open intake destination. The schedule grants no tools,
account access, approval authority or private knowledge. Scheduled work enters the
existing pooled background queue and ordinary controlled execution paths.

## Behavior

- Explicit creation reviews the destination, task and business-timezone timing.
  Exact request receipts survive lost acknowledgments and browser reload. Creation,
  reads and due checks never call a model.
  Display background execution unavailability when this controller is not pooled.
- A controller tick commits one occurrence, its source message/reply/background
  job and next due cursor in one workspace transaction. Reuse common intake and
  its queue bounds. Concurrent controllers cannot create duplicate occurrences.
- Coalesce missed fixed intervals to the latest due instant without replay bursts,
  anchored to the first UTC instant. One unfinished task per routine, including
  question/approval waits. Full queues retain due work without cursor advancement.
- Keep immutable schedule configuration. To change instructions/timing/destination,
  pause and create another. Exact lost creation acknowledgments remain replayable.
  State changes use saved request IDs and revisions; stale writes are rejected.
  A recurring occurrence advances only its timing cursor, so ordinary due ticks
  do not make an otherwise current human Pause request stale.
- Source-visible maintainers may inspect runs and pause a routine; only its original
  principal may explicitly resume using their current authority. No person silently
  borrows another's principal. Read access never implies permission to request work.
- Pausing cancels unstarted initial queued occurrences; running tasks and already
  saved question/approval continuations retain exact task Stop. Explain the distinction.
  Stop one run never disables future distinct occurrences. Completed/failed/stopped
  occurrences are never recreated or automatically replayed by the scheduler.
- Permission, membership or destination changes block future occurrences durably;
  regrant/restore does not silently reactivate a blocked routine. Existing accepted
  work follows its own current task authority and independent approval checks.
- Show bounded source-authorized run history, actual task states and original Chat
  links. Preserve personal drafts, denied/stale state clearing, keyboard/focus,
  phone/short-window and light/dark behavior. No paid model/provider/cloud calls.

## Implementation and acceptance order recorded before edits

1. Inspect canonical launch authority and existing private scheduling/intake paths;
   save/inspect OpenPencil desktop/phone concepts before changing UI.
2. Extract the existing transactional background-intake helper without changing
   interactive/replay behavior; add schema 27, immutable schedules, exact state
   receipts, occurrence dedupe and bounded current-authority due admission.
3. Own controller polling/shutdown and reuse pooled workers. Add source/lifecycle
   invalidation so role/grant/channel revoke-and-restore cannot reactivate routines.
4. Add React Automation list, guided create/review, pause/resume and run history.
5. Prove replica duplicates, queue pressure, coalescing/clock rollback, overlap,
   role/source/agent changes, pause/Stop, missing pooled mode and restart. Execute
   real Goose/scripted-model question → separate exact approval → one confirmed
   operation, including archived/revoked destination and narrow/dark UI.
6. Run focused and relevant existing checks, retain evidence and publish one PR
   after #72. Only then begin connected events or channel handoffs.

Calendar/cron recurrence, external webhook subscriptions, agent-to-agent handoffs,
real fleet delivery/deadline promises and paid-provider/cloud acceptance are separate.

## API, state and execution

Schema **27**, `m20261026_000027_shared_agent_schedules`, adds immutable routine
configuration, exact state-change receipts and unique `(workspace, schedule,
due_ms)` occurrence records. It retains schema-26 profiles and sample receipts.
Apply migrations and runtime grants before startup; see
[PostgreSQL setup](postgres-workspaces.md). No normal workspace DB was migrated
by the local implementation checks.

Under `/api/identity/workspaces/{w}/agent-profiles/{a}`:

- `GET /schedules` returns source-visible saved routines with 240-character instruction
  previews, up to 100 current eligible
  destination conversations, and pooled-mode availability. The agent has a current
  operational guard of 100 stored routines, matching the earlier local proof;
  these are technical bounds, not subscription concurrency tiers.
- `POST /schedules` accepts `id`, `chat`, `name`, `content`, `first_ms`, optional
  `interval_seconds` and `timezone`. The name is at most 100 bytes; instructions
  at most 10,000 bytes. New first runs must be in the future through 2100; fixed
  intervals accept 1–31,536,000 elapsed seconds. The UI offers minutes, hours and
  24-hour periods and reuses existing business-time/DST validation. The timezone
  comes from the current scoped Home preference or explicit Device time fallback;
  the saved UTC instant and timezone label do not change when the device travels.
- `GET /schedules/{id}` loads complete saved instructions only when opened, with
  fresh source and maintainer authorization.
- `POST /schedules/{id}/state` accepts an exact request UUID, current state revision
  and `enabled`. A visible maintainer may pause; only the original principal may
  resume with current authority. An exact older acknowledgment never re-applies
  its state after later changes. A completed one-time occurrence cannot restart.
- `GET /schedules/{id}/runs` returns the last 20 accepted occurrences with current
  task/progress state and source Chat links. Reading does not claim execution,
  continue a question, approve an operation or make a provider request.

Enabled, paused, blocked and completed are routine states. Completed means that
one one-time occurrence was accepted; its task may still be queued, waiting,
failed or cancelled. Individual task outcomes remain separate. Queue pressure is
shown explicitly while preserving the due cursor. An unfinished run is shown in
the routine and prevents new occurrences until its existing task is terminal.

Each controller polls once per second, considering up to 20 due candidates.
A checked timestamp rotates pressured candidates; workspace transactions and the
unique receipt serialize admission across replicas. The existing background limit
of 32 pending requests and total limit of 64 remain intact. A nested transaction
rolls back rejected intake before marking invalid saved references blocked, so a
bad mention cannot partially publish a request or stall unrelated routines.
No calendar parser, second business queue or new dependency was introduced.

Role/removal changes retain receipts and block enabled routines. Source/grant,
agent pause and archive changes check standing authority in their workspace
transaction. Restoring access does not re-enable blocked routines. Source access
revocation permanently cancels affected accepted scheduled tasks through the
existing task-revocation path; archive alone preserves already accepted work.
The controller scheduler owns no model processes: pooled workers execute ordinary
controlled jobs, release on durable questions/approvals and keep their existing
lease/recovery behavior. The scheduler itself never recreates a saved occurrence.

The client lazy-loads Automation, keeps form drafts on failed saves and preserves
an uncertain exact creation payload in user/workspace/agent-scoped session storage.
Retry sends that exact payload. Explicitly editing as a new schedule gets a new ID
and leaves any earlier saved routine unchanged; the UI asks the person to inspect
saved routines first. Revocation clears open private details before revalidation;
late save acknowledgments cannot reopen or refresh an unmounted agent view.

## Design and checks

[The design record](../designs/shared-agent-schedules/README.md) contains the
editable OpenPencil concept and inspected desktop/phone vectors/renders. Actual
local application captures accompany executed browser acceptance.

`cargo test --locked --lib automation_ -- --ignored`, with a disposable
`RELAY_TEST_DATABASE_URL`, checks duplicate controllers/creation, coalescing,
backpressure, rollback, exact state receipts, pause versus continuation, source
and membership revoke/regrant, archive/restore, private agents, unavailable mode,
wrong authority and the populated-v26 migration. The ordinary anchored-time test
runs without PostgreSQL. `web/tests/agent-automation.test.mjs` checks explicit
review/writes, lost acknowledgment/reopen, double save, DST rejection, role denial,
late acknowledgment and synchronous source clearing.

`node --test tests/agent_automation.mjs` runs real pinned Goose against a scripted
local provider and an authenticated MCP fixture. Its scheduled task asks an
ordinary question, releases execution, receives a separately approved operation
and confirms one business-fixture write. Browser/controller restart, Stop,
exact save recovery, private source revocation and phone/dark/short windows are
included. These are local protocol and UI checks, not commercial-model quality,
actual distributed cloud delivery, capacity, deadlines or paid-provider acceptance.

The final local check passes 97 Rust unit, 184 standalone PostgreSQL, 147 web and
52 offline Python tests, Clippy, Rust/web builds, browser-test types and all
756 checked source files under the 500-line limit. Ten unique connected
process/browser journeys pass; a final repeat waits for the full loaded phone
review before checking 390×844 and 320×600 bounds. The scheduling flow records
nine scripted model requests for its explicit question/approval continuation and
one confirmed MCP-fixture write. Status/review/restart reads add no inference.
Evidence and earlier failed checks are retained in
`.tmp/shared-agent-automation/verification.json`.

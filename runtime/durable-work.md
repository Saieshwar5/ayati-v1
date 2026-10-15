# Durable work: first L3 increment

The local Rust daemon uses SQLite/WAL with FULL synchronous commits for accepted
inputs, a bounded persisted queue, run ownership, decisions and action observations.
No queue server or additional service is introduced. One local agent runs one job
at a time; concurrency and business priorities remain future product decisions.

## Acceptance and ownership

`POST /api/messages` accepts `content` and an optional UUID `request_id`. Within
the fixed local conversation, one receipt ID identifies one exact content payload.
The receipt, user message, task, scope and initial transition commit together.
Only then is success returned. An identical retry returns the original task even
when the queue is full or the task has finished; changed content returns 409.
New work beyond 32 queued/running jobs returns 429 without a partial receipt.
Omitting the ID creates a fresh job and provides no retry deduplication.

The browser retains one unacknowledged draft/ID in tab session storage, with an
in-memory fallback when storage is unavailable. It never automatically resends.
After acknowledgement the ID is cleared; edited content is a new request. Storage
is local to the current web origin; multi-business/member draft scope is later work.

The worker claims the oldest queued job in a SQLite transaction before contacting
Goose. A saved claim identifies the owning process and an opaque execution token.
Notify only wakes the worker; losing a notification cannot erase an accepted job.
Reply writes, tool observations and approvals require the current active claim.
ACP callbacks are also bound to their originating connection, so a disconnected
runtime cannot attach events to a later job in the same saved session.

The daemon's exclusive data-directory lock remains required. These claims fence
local execution; they are not distributed leases, automatic failover or a promise
of exactly-once external effects. Database failure while finishing a claim parks
the worker and rejects further submissions until restart instead of risking new work.

## Restart and Stop

| Situation | Result |
| --- | --- |
| Accepted but never claimed | Remains queued and dispatches after configured restart unless explicitly paused |
| Runtime/provider unconfigured | Queue stays parked; no model call or VM boot |
| Claimed job interrupted by crash/shutdown | Marked interrupted; no automatic prompt replay |
| Owner stops a queued/running job | Stopped permanently; late results cannot revive it |
| Service shutdown or SIGTERM | Interrupt active work, invalidate pending approval, preserve queue |
| Explicit agent VM Stop | Stop both active and queued work, persist dispatch pause, then shut down the VM |
| Legacy queue from schema 1/2 | Mark interrupted during migration; no new automatic replay |

Queued work may now boot the guest and make billed model calls at startup.
Only never-started jobs qualify; session loading never proves safe action replay.
An owner may send deliberate follow-up for interrupted work. Completed effects
remain completed even if a later Stop or connection loss occurs.

## Action evidence

`GET /api/tasks/{id}/actions` returns recorded tool identity, input, status,
owner decision and bounded output, with at most 128 records per run. Inputs are
bounded to 16 KB; retained outputs are capped at 64 KiB with explicit omission.
Goose records have `evidence: runtime_observation`; preserved prototype records
have `evidence: prototype_tool`. Private reasoning is excluded.

Permissions link the exact observed call to its run token. A rejected call remains
denied; an unresolved approval is cancelled on Stop/restart. A tool that may have
started without a confirmed result becomes uncertain. A model EndTurn cannot mark
the job completed while such an action remains unresolved. Transition history and
known results remain saved; stale events cannot overwrite terminal records.

These observations are not third-party business receipts. The lost-reply test
records intent/approval, commits an independent synthetic service write, loses its
reply, restarts storage and verifies an uncertain action with no replay and one
actual synthetic delivery. Real Goose/MCP checks separately verify read-tool
allow/deny and recorded output. No real business connector/write or automatic
outcome reconciliation is implemented; those require service-specific operation
IDs, idempotency support and trusted receipt checks.

## Verification and next work

`tests/durable_work.rs` covers migration, bounded acceptance/retries, ownership,
Stop, unknown outcomes and the independent simulated write. The shared Goose
fixture runs real pinned Goose/MCP, with explicit scripted model responses.
`tests/goose_durability.mjs` checks concurrent duplicate HTTP delivery, crash queue
recovery, shutdown/approval invalidation, unconfigured restart, and a real browser
retry after the server accepted the message but its HTTP acknowledgement was lost.
Both host and real-VM selections are explicit; use README commands.

Current local results: 32 Rust tests, 10 web tests, 10 real-Goose host scenarios,
13 real-VM scenarios, and the 4 new durability scenarios again on the final release
VM path all pass. The 8 existing prototype browser journeys and HTTP smoke pass,
as do fmt, clippy, web/e2e type/build and source-length checks. These are local
fixture checks, not live AI/business or hosted CI results.

The following L3 increment adds [scheduler/event receipts, local active-work
leases and safe idle release](scheduled-work.md), upgrading the database to schema 4.
The counts above record the first L3 increment, before that addition.
No live provider quality, multi-user access, production isolation, power-loss
hardware durability or cloud failover is established. Browser ownership and
connector reconciliation remain later increments. Schema 4 rejects newer databases; use backups and a tested
migration process rather than opening upgraded state with older binaries.

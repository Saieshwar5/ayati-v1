# Durable task waits for business approvals

The original PostgreSQL-managed agent task now survives its approval wait. After
proposing an Ask action, Goose explains what is waiting and what remains, then
ends its bounded attempt. Once the harness has stopped, the controller saves a
checkpoint and releases execution. Another task can use the same agent. A saved
decision and confirmed action outcome make the original task eligible for a fresh
attempt, including on another executor after a controller restart.

This extends [channel approvals](channel-approvals.md) and the
[exact action gateway](shared-agent-control.md). This slice introduced schema 22;
current startup additionally requires [ordinary questions](agent-questions.md)
and [Chat organization schema 24](chat-organization.md).
No new provider, dependency, paid inference or cloud resource is required.

## What people see

The existing agent reply stays in its conversation/thread with its original
requester and task identity. Its status changes to **Waiting for approval · Worker
released**. The person can inspect the saved request through its channel card or
**My approvals**, or use **Stop reply** on the original task.

- Approve dispatches only the exact saved action. Once its result is confirmed,
  **Decision saved · Waiting to continue…** indicates the queued continuation.
- Reject and Request changes continue with that distinct decision and no business
  effect. A changed proposal must pass the gateway and obtain fresh approval.
- **Needs attention · External outcome not yet confirmed** holds continuation
  when an action is executing or uncertain. It never repeats the action to find
  out what happened. Existing uncertain-outcome receipts/write holds remain.
- Stop permanently cancels this task and invalidates its pending actions. It
  retains the receipts of effects already dispatched, including uncertain ones.

Opening, reloading or repeatedly approving a card does not create another write
or another task continuation. No model call, worker lease or active slot is held
while waiting for a person. This makes no claim that idle fleet capacity is free.

## Task, attempt and checkpoint

`relay_chat_jobs` keeps its original payload and reply identity. Its lifecycle is
`queued → running → waiting → queued → running → finished`. A job has a monotonic
assignment generation and a separate continuation cycle. Recovery attempts remain
bounded to three per cycle; human waits do not consume those recovery attempts.
The existing 20-operation task bound also bounds approval cycles.

`relay_task_checkpoints` records one checkpoint per task/cycle, with the exact
finishing claim, bounded response text and timestamp. Actions record their task
cycle and whether they require approval. The controller derives these fields;
neither the model nor a decision payload can select a different task or cycle.

The checkpoint transaction verifies current claim ownership and source access,
saves the checkpoint, finishes that attempt and sets the task waiting. A shared
database function either leaves it waiting or queues it once all saved actions
have settled. The same function runs in the action transition transaction:

1. A decision/outcome arriving before checkpoint release is observed at release.
2. A decision/outcome arriving after release wakes the saved task atomically.
3. Duplicate updates find an already queued/running/finished task and cannot wake
   it again. Stop prevents the function from queuing the task.

The worker completion receipt still describes the bounded **attempt**, which can
finish while its task is waiting. Controller-scoped completion replay and slot
generation fencing retain their existing protocol. Old attempts cannot renew,
write output, request tools or publish a late result into a resumed task.

## Reconstructing a fresh Goose attempt

This is reconstruction from saved task data, not transfer of an in-memory model
process. The controller prepares fresh authorized context containing:

- The unchanged original request and its workspace/agent/conversation/thread.
- The latest checkpoint explaining completed and remaining work.
- Saved exact actions, decisions, results and receipts for this task.
- Current agent configuration and independently authorized Library/reference data.

Checkpoint text, reference notes and external results are explicitly untrusted
data. They confer no permission. The gateway still deduplicates an exact action
within the original task, so replaying its completed write returns the saved
receipt. A changed operation/argument set is a new action subject to current rules.

Checkpoint text is bounded to 64 KiB and saved continuation data to 128 KiB, within
the existing prepared-context limit. Oversized data fails explicitly rather than
silently dropping decisions or receipts. These are engineering bounds, not new
subscription entitlements. Hidden model state and arbitrary local scratch/files
are not serialized into the checkpoint.

## Authority, cancellation and recovery

Current requester, workspace, agent, conversation membership, Library source and
running-claim access are checked before context delivery and tool use. Approval
retains its separate designated-person/account-operation authority and exact
final-dispatch checks. A decision does not restore revoked task access.

Source membership/settings changes, request-grant withdrawal, workspace membership
changes, agent pause and Library sharing changes cancel saved tasks that lose
access in the same transaction. Rejoining, unpausing or regranting access cannot
revive those cancelled tasks. Existing Library access revisions also fence old
sources. Revocation cannot undo an external effect already dispatched.

If a worker disappears before checkpointing, normal bounded claim recovery can
recover the same proposal identity. If a resumed worker disappears, its replacement
uses the same saved checkpoint and receipts. An executing/uncertain action holds
continuation across restart; no automatic reconciliation or external retry is
introduced. Inspect the remote system before resolving an unknown outcome.

## Upgrade and verification

Explicitly apply `m20261026_000022_durable_approval_waits` using the existing
administrator migration/grant procedure. Stop old binaries first and back up
identity data. Existing job generations are initialized from their attempt count;
old finished proposal jobs are not retroactively resumed. Normal startup checks
the schema and does not apply migrations. Testing uses disposable databases only.

`tests/durable_approvals.mjs` runs real pinned Goose, a scripted local model,
authenticated MCP with independent business records, PostgreSQL and Playwright.
It verifies worker release, another same-agent task, idle model-call counts,
controller restarts before/after approval, continuation on another executor,
exact-write replay, remaining work, Request changes, Stop and uncertain outcomes.
Database tests cover decisions racing with release, multiple approvals, recovery,
more than three continuation generations and revocation/regrant. Browser checks
cover desktop, phone and dark mode; source evidence is retained under `.tmp/`.

This proves the local approval-wait slice of L2. Ordinary choice/form waits now
compose with it through [agent questions](agent-questions.md). Broader event waits,
every-agent/local compatibility, arbitrary artifact reconstruction, automatic
uncertain-outcome reconciliation, progress inquiries with cross-task context,
commercial model/provider quality, multi-day soak, usage/billing and cloud
capacity/recovery acceptance remain separate work.

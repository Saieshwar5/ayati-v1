# Channel-visible agent handoffs

Implemented locally on `feat/channel-agent-handoffs`, serially following PR 74.
This contract was saved as a plan before application edits. The acceptance section
records executed local behavior and the remaining provider/cloud boundary.

## Product and authority contract

A running workspace agent can explicitly ask another managed workspace agent for
help in the same existing channel and original thread. Ordinary chatter does not
start work. There is no private direct-agent route or cross-channel publication.
A human's original request remains the principal: both agents must participate and
that person must have current independent request permission for each. Agent
configuration, membership and visibility never become implied business authority.

The controller discovers eligible helpers and accepts a stable-key handoff through
the exact current task capability. It atomically saves an agent-authored channel
message, linked child task and immutable parent/root/principal/thread receipt.
Retries reuse the exact saved handoff, including after worker/controller loss;
changed contents with an accepted key fail. Identical target/instructions with a
new model-generated key return the original canonical receipt, preventing worker
recovery from manufacturing another helper task. Root-to-descendant loops, self-handoffs,
more than three levels, more than two direct children, more than eight total child
tasks and unbounded waits are rejected. `AYATI_HANDOFF_WAIT_SECONDS` defaults to
seven days (60 seconds to 30 days supported); a descendant cannot extend its
root handoff deadline. These are operational controls rather than
new subscription entitlements. One unfinished child per sender at a time keeps
coordination understandable; completed siblings remain inspectable.

Receiving agents use only their own current configured tools and knowledge. A
business operation must also fit each sender's snapshotted and still-current
operation rule along its lineage. Missing/Block denies; an ancestor's Ask ceiling
requires an independent receiver Ask rule with the same designated approver. The
controller does not silently broaden or rewrite a receiver's standing rules.
Final dispatch and approval decisions recheck these ceilings. Uncertain writes
retain their ordinary receipts and cannot be retried by changing agents.

## Source publication and retention

Handoff instructions are a bounded explicit message, not a copy of the sender's
private prompt, notes, credentials or transcript. Only the existing channel and
thread audience can see them. Private-agent/private-DM/Library-source handoffs are
outside this first channel slice; reject rather than publish across that boundary.

Connected-event source data may be passed only when each receiving agent has its
own effective Allow read for the same reviewed source/account/destination and the
original person retains standing read authority. Save receiver read versions with
the handoff. The bounded task tree retains those source requirements: derived
parent/child output, questions, approvals, readers, attention and progress remain
source-checked after a result is returned. Source revocation cannot leak through
a parent's eventual answer. Scope-aware live invalidation clears affected content.
Independent later conversation prompts exclude event/handoff-derived messages;
continuation uses the exact original-task receipts and authorized child result.

The source requirements cover the whole bounded tree after a result has returned,
so a parent's later answer cannot bypass a helper's event-read revocation. Pin
source/read versions for every receiving agent and emit the existing source/chat
invalidation when one changes. The original event remains retained while the parent
waits; normal event cleanup can erase input after the tree is terminal. Explicit
handoff messages and root/child replies are excluded from unrelated future prompt
assembly. Ordinary channel history remains readable under its own source checks.

Operation ceilings are captured from the sender's effective rules at admission
and rechecked against current versions along the ancestor path. Receiver actions
may only be equal or more restrictive; incompatible Ask approvers deny that action
rather than silently assigning new authority. The child still has independent
exact action receipts, approvals and uncertain-write holds.

## Asynchronous lifecycle

After requesting help, the sender reports what is waiting and finishes its bounded
attempt. Save its checkpoint and release the worker. A child may complete before
or after that checkpoint; either ordering queues the same parent continuation once.
The parent resumes from current authorized receipts, never by polling a model or
starting another independent business task. Human questions and exact approvals
compose with this wait; status/history reads make no model calls.

Show sender, receiving agent, original requester, current state and links to the
parent/child in the original channel/thread and Agents Work view. Stop a child
returns a cancelled outcome to the parent. Stop a parent/root cancels unfinished
descendants; restart or redelivery cannot revive them. Source/request/participation
revocation permanently fences affected work. Archiving blocks new handoffs while
preserving accepted continuations. Expiration is recorded as a terminal outcome,
not a silent automatic retry. No cancelled/uncertain external effect is presented
as undone.

## Implementation sequence

1. Inspect the current channel/control/continuation boundaries and live OpenPencil
   documents; save desktop/phone channel-handoff concepts before UI edits.
2. Add schema 29 receipts, bounded atomic child admission and saved wait/resume;
   reuse the ordinary pooled background intake and current worker capabilities.
3. Add shared source/ceiling guards and lifecycle propagation across Stop,
   revocation, completion, checkpoints and original-source disclosure paths.
4. Expose understandable channel/Work references and original-task navigation;
   preserve loading/error/revocation, keyboard, draft and small-window behavior.
5. Prove two-controller exact handoff, self/cycle/fan-out limits, no privilege
   amplification, parent/child Stop, source revocation including returned output,
   result-before-checkpoint and checkpoint-before-result, queue rollback,
   restart/reconstruction and one separately approved MCP-fixture write using real
   pinned Goose and scripted local inference. Retain records and actual captures.
6. Run focused plus risk-appropriate regression checks and publish one dependent
   PR after #74 before starting another feature. Never migrate the normal DB or
   activate cloud/paid inference.

A2A networking, cross-channel/private-agent delegation, arbitrary source migration,
parallel filesystem/browser execution and real cloud/provider acceptance remain
separate work. This plan does not promise launch completion or production capacity.


## Runtime and local verification

Schema **29**, `m20261026_000029_channel_agent_handoffs`, adds immutable handoff
receipts, source/permission snapshots, bounded wait metadata and terminal outcomes.
The populated-v28 migration preserves an existing pending question and resumes
its same original task after an answer. Apply explicit migrations/grants only to
the intended deployment; current startup follows [PostgreSQL workspaces](postgres-workspaces.md).
The normal workspace database was not migrated by these checks.

`handoff_targets {}` and `request_handoff {key,agent,task}` use the existing exact
per-task worker capability and controller-owned MCP bridge. Users/models cannot
supply another principal, parent, workspace, channel, thread or permission set.
Shared pooled execution is required. Other adapters fail without admitting work.
The UI requires no invented connector grants or separate handoff permission form.

A helper result enters its parent's continuation as at most 2,000 characters with
an explicit `truncated` marker and original source link. The complete saved child
response remains available through its authorized reader; the original result is
not shortened in storage. Truncated/stopped/failed outcomes do not establish that
an external effect succeeded. Completed and uncertain operation receipts are
shared across the logical task tree so changing agents cannot replay the same
operation/fields as a new effect.

A helper task must checkpoint before the sender starts more work, even if the
helper finishes quickly. Both completion/checkpoint orderings queue exactly one
continuation. An owned bounded controller loop checks at most 20 pending roots
per pass with fair rotation; it records expiry without idle inference. Controller
replicas serialize only the necessary workspace mutations. These local controls
are not throughput, cost or cloud-deadline acceptance.

```mermaid
flowchart LR
  H[Human request in channel] --> P[Original agent task]
  P --> R[Saved handoff receipt and visible thread message]
  R --> C[Helper task with original human and independent controls]
  P --> W[Checkpoint and release worker]
  C --> D[Saved result or terminal outcome]
  D --> N[Resume the same original task once]
  W --> N
```

Verified locally:

- 101 root Rust unit and seven guest-bridge checks, 216 disposable PostgreSQL,
  157 web and 52 offline Python checks.
  Eighteen PostgreSQL handoff cases cover exact and semantic replay across
  replicas/new model keys, queue rollback, self/cycle/depth/fan-out limits,
  immutable deadlines, expiry, both checkpoint/result orderings, Stop, archive,
  membership/regrant, missing/Ask/Allow ceilings, no delegation around unresolved
  input/approval/uncertainty, whole-tree exact operation reuse and returned-source
  privacy including handoff request readers.
- Real pinned Goose with scripted local inference hands work to Atlas, releases
  Nova, asks the original person a question, survives controller/executor restart,
  requests separate exact approval, performs one confirmed authenticated MCP
  fixture write, then resumes Nova from the saved result. Child and parent Stop
  terminate the correct linked work; another restart starts no replacement work.
- A second real Goose journey passes a signed event to a helper with independent
  Allow read authority, returns its saved result to the parent, then revokes the
  helper read. The open parent reader and all source-bearing parent/child/request
  reads become unavailable; regrant/restart cannot revive them. This journey
  performs zero MCP calls. Status/history reads make no model calls.
- Desktop, phone, dark and source-reader captures were inspected. Web behavior
  checks cover guarded Stop, failed saves, exact navigation and immediate scoped
  clearing while preserving an unrelated chat draft. Denied reader retries now
  leave a usable error state instead of a stuck loading indicator.

Complete regression/process totals and check artifacts are retained in
`.tmp/channel-agent-handoffs/verification.json` before publication. No paid model,
cloud resource, provider subscription or production/cloud acceptance was used.
Private-agent/DM/Library delegation, cross-channel publication, richer result
retrieval, A2A networking and concurrent per-agent computer/filesystem work remain
separate increments. Direct agent-to-agent routing is outside the accepted launch
scope. The whole product and launch acceptance contract remain incomplete.

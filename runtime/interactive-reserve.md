# Chat reserve and background admission

The pooled controller can hold compatible ready capacity for human chat while
background requests run. It reserves admission headroom, not a worker for each
agent. Idle agents still own no process. The shared ready fleet remains paid
capacity, and every used Goose process/session is replaced after its one use.

## Configuration and boundaries

Set `AYATI_EXECUTOR_CHAT_RESERVE` on **every pooled controller**. The default is
`0` for compatibility; a positive value enables reserved headroom. It must be
smaller than the global, workspace and provider/model limits. For example:

~~~sh
AYATI_CHAT_EXECUTION=pooled AYATI_EXECUTOR_CAPACITY=4 \
AYATI_EXECUTOR_WORKSPACE_LIMIT=4 AYATI_EXECUTOR_MODEL_LIMIT=4 \
AYATI_EXECUTOR_CHAT_RESERVE=1 cargo run --locked -- identity
~~~

These are illustrative policy values, not AWS sizing recommendations. Provision
compatible executors separately using the [pool guide](executor-pool.md). All
controller replicas must use the same policy. Invalid bounds fail startup.

A background offer is allowed only when, after admission:

- The global active limit retains the configured reserve.
- That workspace and provider/model retain the same admission headroom.
- The compatible, prepared, serving slots on currently connected executor
  identities retain the configured number of ready slots.

Preparing, unverified, incompatible, draining and disconnected capacity cannot
stand in for ready capacity. Connection membership is a live hint; PostgreSQL
still owns claims/generations/leases. Disconnects can race a snapshot, and lost
capacity requires recovery. The reserve is not an availability guarantee.

For example, with four configured runs but only one connected two-slot executor,
a reserve of one permits at most one background run. The remaining slot can serve
chat. Existing owners are never killed to create reserve. A replacement fleet
may temporarily have fewer ready slots while processes prepare.

Interactive requests can use the whole pool within the existing limits. A burst
larger than available capacity still queues. The one-active-run-per-agent rule
also remains: a message to an already busy agent waits for that agent, even if a
different agent could use the reserve. Safe interruption/checkpointing is later
work. The reserve does not bypass membership, source access, grants or budgets.

## Explicit background intake

Normal chat and Library questions are classified as `interactive` by the server.
The new authenticated endpoint accepts a deferred, explicit agent request:

~~~text
POST /api/identity/workspaces/{workspace}/chats/{chat}/background-requests
{"id":"request-uuid","content":"Prepare this summary","agent":"agent-uuid"}
~~~

It uses the same session, origin/request guard, conversation membership, agent
grant, attachment/thread checks and content bounds as ordinary chat. It requires
PostgreSQL pooled execution and an explicit agent. The response includes
`work_class: "background"`; the request and queued reply appear in normal history.
This is an API capability; no new frontend control is added in this increment.

Class is fixed by the route, outside the model prompt/payload. The ordinary
`NewMessage` schema rejects client-supplied class/priority fields. An exact replay
is idempotent; sending its ID through a different class route fails. A database
trigger prevents changing accepted classification or request-message binding.
Later start/output/final operations still recheck current authority. A queued
background request does not preserve a revoked grant.

Background intake is limited to 32 pending replies per workspace within the
existing 64-reply total bound. Excess background submissions receive HTTP 429
without a partial message/job. This prevents background work alone from filling
all pending-request space. Interactive traffic can still fill the total queue;
no unbounded intake is introduced.

Local agent schedules still use their separate local runtime. This does not
silently migrate them, enable remote business tools, or add cloud schedule/event
services. Future trusted trigger adapters can use the background intake boundary
with an explicitly authorized requester; machine authentication comes separately.

## Fairness and persistence

Chat is normally preferred. After three interactive offers for a provider/model,
one eligible background request receives the next turn. Background eligibility
still requires all reserve/quota checks above. The turn counter is durable in
PostgreSQL, updated in the same admission transaction as the offer, and survives
controller replacement. If no background request is eligible, chat continues.
If chat is absent, eligible background work need not wait for three chat offers.

This is bounded admission fairness, not a deadline or execution preemption.
Existing workspace fairness and per-agent ownership apply within each class.
Capacity, compatible readiness, model limits and current access can still defer
work. Failed/lost offers retain the existing fenced recovery behavior.
Owned model quota is counted from the assigned slot/profile (or saved context
for a cold assignment), so pausing/editing an agent cannot free that quota before
its old run releases ownership.

## Migration and rollout

Run all migrations through `m20261024_000017_cloud_executors` with the migration
owner, then reapply restricted runtime grants. Startup only checks the schema.
Migration 16 adds immutable request class/binding and durable admission turns.
Historical chat/Library jobs default to interactive; existing IDs, payloads,
attempts, deadlines, credentials and completion fingerprints remain unchanged.

Upgrade/drain all older controllers before enabling background intake or a
positive reserve. Older dispatchers do not enforce this policy; mixed controller
versions are not supported for its activation. Existing owned work can finish
under its accepted lease. The existing dispatch-mode and forward-migration rules
in [PostgreSQL setup](postgres-workspaces.md) still apply.

## Reproduce

Build Ayati and the [prepared Goose artifact](harness-preparation.md), freeze an
immutable binary, then run the existing private-cluster resource-bounded runner:

~~~sh
python3 tools/run-agent-load.py --suite mixed --executor-cpu 200 \
  --binary /absolute/path/to/frozen/ayati
~~~

The mixed suite sends real, scoped Goose requests to a local scripted provider.
It holds background model calls while repeated small/large chats use the reserve,
keeps background backlog unclaimed, then releases it and checks completion. Other
journeys cover authenticated intake/backpressure, a smaller fleet, active drain,
revoked grants, controller restart and actual executor loss/lease recovery.
Reports retain per-request traces, distributions, target flags and resource limits.
The ready-capacity timings exclude waiting for replacement processes to prepare;
capacity recovery and overflow remain separate observations.

Container execution requires working engine access. Native release-binary checks
are separate evidence and do not validate the Docker image, cross-host transport,
AWS identity/scaling or live-provider first-token latency.

## Native release evidence

The optimized Ayati release ELF is SHA-256
`3ad52e76544059fac61194bf1edeb50104359d5726acdfb5775dc5965621b1f3`, built with
`cargo build --locked --release --bin ayati`. Goose retains the separately pinned
prepared artifact from [harness preparation](harness-preparation.md). Every final
process suite uses the same immutable copies; the earlier debug-build timings
are historical and are not an isolated before/after comparison of this policy.

The mixed suite passed all five journeys in `.tmp/bounded-agent-c3tcjm3u`.
Its main workload held three background calls plus one queued background request
while 96 small/large-context chats ran in four 24-sample cases (two repetitions).
Every chat and all four healthy background requests had one inference/attempt;
the held owners were not preempted. All four ready-capacity cases met the existing
provisional targets. Dispatch p95 was 6–16 ms; model-request p95 was 51–194 ms.
The request mix is deliberate provider waiting, not arbitrary CPU-heavy background
work. Per-role quotas and shared-host limitations are the same as the
[bounded runner](bounded-agent-load.md).

The other journeys passed authenticated intake/class-spoofing/replay/backpressure,
actual two-slot availability despite a four-run configuration, graceful drain,
pre-start grant revocation, controller replacement and real executor loss/lease
recovery. Revoked pre-start requests retain the existing denial/retry/exhaustion
behavior and finish without inference or a published draft; this increment does
not introduce a new cancellation protocol.

Native checks passed: 149 default Rust tests, 84 disposable PostgreSQL checks,
strict all-target Clippy, release build, four Python resource/cleanup tests (also
under optimization), format/diff/link and source-file length checks. Initial test
harness failures and an intermediate native run remain retained; final evidence
is indexed under `.tmp/interactive-reserve-verification`. Docker access was
unavailable during that native increment. The later
[release-image verification](release-image-validation.md) now passes the same
mixed profile and two ordinary matrices with the actual packaged artifacts.
Ordinary application databases, AWS
resources and paid models were not changed/used by this increment.

All 33 primary process journeys pass on that identical release/Goose pair: five
mixed, two ordinary matrices, three preparation, three fault and twenty existing
browser/stream/pool/cold-worker/HTTPS/Library/permission journeys. The mixed main
fixture is `.tmp/independent-worker-v2mxzG`; the complete
[bounded guide](bounded-agent-load.md#native-release-with-chat-reserve) indexes the
remaining runs. Final auditing verifies that every owned scope/private postmaster
is stopped. No runtime or schema changes were applied to ordinary installations.

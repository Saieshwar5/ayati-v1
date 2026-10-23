# Local assignment workers

M1 runs PostgreSQL-managed, tool-free conversation assignments in temporary
processes. A local launcher records an attempt, starts one worker for it, observes
exit and reconciles failures. Workers renew ownership, save an outcome and exit.
The launcher remains available with zero workers while agents have no work.

This is a local execution foundation, not an AWS deployment or resumable business
agent. Goose remains pinned at 1.54.0. The original continuously polling
[worker mode](independent-chat-worker.md) is retained.

M1.5a's [controller-scoped API path](worker-control.md) now offers the same local
assignment lifecycle without child-worker database credentials. This document
describes the retained direct-PG compatibility mode. M1.5b adds the opt-in
[prepared executor pool](executor-pool.md) with separate pooled dispatch.

## Migration and startup

Run all migrations through 16, m20261024_000016_interactive_reserve, with the owner connection,
then reapply runtime table/sequence grants. See [PostgreSQL setup](postgres-workspaces.md)
and [hosted packaging](hosted-deployment.md). Startup never migrates ordinary data.
Existing jobs remain polling jobs; they are not silently converted to assignments.
Stop/drain old binaries and queued polling work before switching modes. Old
binaries do not implement the new admission boundary. All API replicas accepting
assigned work must use the same mode and compatible schema.

Configure RELAY_DATABASE_URL privately for the API and launcher. WorkOS credentials
belong only to the API; supported model credentials belong to the launcher/workers.
Do not put actual secrets in example command arguments. Start the API:

~~~sh
AYATI_CHAT_EXECUTION=assigned AYATI_WORKER_PROVIDERS=openai \
  cargo run --locked -- identity
~~~

Start the local launcher with its own private data directory and provider setup:

~~~sh
AYATI_RUNTIME=goose AYATI_CHAT_EXECUTION=assigned AYATI_GOOSE_PROVIDER=openai \
  AYATI_DATA_DIR=.ayati-local/assignment-launcher \
  cargo run --locked -- assignment-launcher
~~~

The launcher resolves the pinned Goose binary before entering the child directory.
It allowlists child environment settings, omits WorkOS/unrelated AWS credentials
and local browser/VM settings, and blocks parent .env discovery in that directory.
Model keys still reach the configured Goose provider; this is not a per-run model
credential/budget broker. No developer AWS profile is used.

After persisting a launch intent, the launcher starts:

~~~text
ayati worker --assignment <assignment-id>
~~~

This ID identifies a reserved attempt, not an arbitrary chat reply or bearer
credential. Unknown, duplicate, expired, finished and wrong-resource starts fail.
Do not configure an always-restarting supervisor for completed assignment commands.
Supervise the launcher instead. The worker does not search for a second job.

## Configuration and ownership

| Setting | Default and bounds | Applies to |
| --- | --- | --- |
| AYATI_CHAT_EXECUTION=assigned | Explicit opt-in | API, launcher, assigned workers |
| AYATI_ASSIGNMENT_RUN_SECONDS | 180; 1–86400 | Accepted assigned job, persisted at intake |
| AYATI_ASSIGNMENT_LEASE_SECONDS | 120; 6–300 | New attempts reserved by launcher |
| AYATI_ASSIGNMENT_CAPACITY | 2; 1–16 | Cooperating local launchers in the same schema |
| AYATI_WORKER_PROVIDERS | Existing supported provider declaration | API display, not a live availability probe |
| AYATI_WORKER_HEALTH_FILE | Optional private absolute path | Explicitly supervised worker process |

RUN_SECONDS bounds one attempt, not total workflow lifetime. Short chats retain
180 seconds. Set an explicit larger value on the API before accepting long work.
Both the execution loop and underlying ACP prompt use the persisted bound. Worker
replacement does not substitute its own duration. At most three attempts run.

Pending launch ownership lasts 300 seconds. Registration resets it to the attempt's
lease; renewal runs about every third of that lease using PostgreSQL time. Expired
ownership cannot be resurrected. Renewal failure stops further execution; result
publication still requires a current claim and fresh access checks.

Dispatch metadata separates polling and assignment jobs. Compatible admission
prevents an assigned job and a polling job for the same agent from overlapping;
legacy-to-legacy parallel behavior is preserved. Assigned mode admits one owner
per agent, regardless of conversation, and enforces the configured active cap.
Cooperating launchers must use the same capacity setting. This short serialized
admission gate is not production fleet fairness or spending enforcement. No SQL
transaction remains open across process spawn, model execution or streaming.

The relay_worker_assignments ledger stores assignment ID, workspace/reply,
generation, provider/resource identity, policy, heartbeat, timestamps and outcome.
Each retry gets a new ID and increasing attempt number. The current job claim
fences stale registration, renewal, publication and release.

## Completion, recovery and cancellation

Normal completion atomically saves the reply and finishes job/attempt records.
Failed persistence rolls completion back and returns a nonzero exit. A saved
execution failure is terminal. Crashes, launch failures and process interruption
may retry up to three times; exhaustion becomes a visible failed reply.
Retries can repeat model inference/cost and are suitable here because there are
no external business effects. They do not resume a checkpointed business workflow.

The local launcher releases ownership after confirmed child exit. SIGTERM asks
workers to stop/reap Goose and release unfinished work; after fifteen seconds it
can force-stop a child. Launcher death triggers the Linux parent-death boundary
for its workers; workers supervise Goose in turn. A replacement launcher uses
durable state and lease expiry. This differs from independent ECS tasks surviving
controller replacement: AWS service placement/reconciliation is M2, and real task
identity/scope proof is M3. Local scoped APIs exist in M1.5a. A PID is diagnostic identity.

Authenticated PostgreSQL conversations also expose:

~~~text
POST /api/identity/workspaces/{workspace}/chats/{chat}/messages/{reply}/cancel
~~~

Use the existing session, same-origin mutation protection and request header.
The actor needs conversation access and must be the requester or workspace Owner.
Cancellation is idempotent, audited, clears ownership and blocks future retry or
publication. It can cancel queued work before any worker starts. Finished results
remain final; cancelling a request does not undo earlier effects.

This endpoint cancels one request. It does not add a client Stop button, pause an
entire agent/schedule or implement approval waits. Process SIGTERM is distinct:
it parks unfinished work for recovery rather than recording user cancellation.
Existing PostgreSQL polling jobs honor the same request invalidation.

## Scope and verification

Direct-PG compatibility workers retain the restricted core PostgreSQL connection.
M1.5a's separate API mode removes it locally; M3 must prove actual AWS machine identity.
Assignment IDs never replace requester/workspace/agent/source authorization.
Every attempt uses fresh private scratch and a tool-free Goose session. No browser,
computer, connector mutation, local shell tool or App deployment is enabled.
Graceful attempts clean temporary state; abrupt crashes can leave private scratch
for controlled cleanup. Attempt history and business records survive worker exit.

General pause/approval resume, receipts for external actions, uncertain-outcome
reconciliation, workspace budgets/fairness and remote tools remain M4 and later.
Context remains the bounded accepted chat/source input, not a new fresh-context or
long-term-memory service. No live cloud/provider acceptance is inferred here.

~~~sh
cargo test --locked --lib
cargo test --locked --lib identity::postgres -- --ignored
cargo clippy --locked --all-targets -- -D warnings
./scripts/check-file-lines.sh
cargo build --locked --bin ayati
node --test --test-concurrency=1 tests/assignment_workers.mjs
node --test --test-concurrency=1 tests/independent_chat_worker.mjs tests/hosted_deployment.mjs
~~~

Set RELAY_TEST_DATABASE_URL privately. Process tests require a dedicated local
relay_simulation_* database. They use real pinned Goose, a loopback scripted model,
restricted runtime DB role and separate API/launcher/worker processes. Coverage
includes mode separation, exit, duplicate registration, renewable ownership,
per-agent exclusion, cancellation/revocation, worker/launcher loss and timeout.
PG tests also cover competing admission, stale fencing, immutable duration, atomic
save rollback, retry exhaustion and upgrading a populated version-10 queue while
preserving polling work. Ordinary databases are not migrated by fixtures.

The default process hold is twelve seconds with a six-second lease. Set
AYATI_ASSIGNMENT_SOAK_SECONDS=185 to exercise the former three-minute ACP limit;
the accepted fixture execution bound is 600 seconds. This is synthetic local
evidence, not a multi-hour real-provider/cloud soak. Fixtures preserve records,
process logs and report.json under the printed private .tmp/independent-worker-*
directory. These checks make no paid model calls or AWS requests.

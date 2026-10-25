# Independent shared-agent worker

[Shared control tools](shared-agent-control.md) now add explicit per-agent/conversation
operations and durable approvals. Tool-free execution remains the default; the
original conversation-only test evidence below retains its scope. Current schema: 22.


[Durable approval waits](durable-approval-waits.md) now add checkpoint/reconstruction
for managed tool-enabled tasks through the controller gateway.

Use [assignment workers](assignment-workers.md) for the newer temporary-worker
mode and local launcher. This guide describes the retained polling compatibility
mode.

Implemented 2026-10-23. This is the first local API/worker separation for
PostgreSQL-managed workspace agents. It is not a hosted deployment or completion
of the remote-tool/cloud-worker plan.

## Startup

Build the current binary and configure a migrated PostgreSQL database privately
using [the existing setup](postgres-workspaces.md). Both processes use the same
`RELAY_DATABASE_URL`. They do not need the same filesystem or data directory.
The original worker increment used schema version 9. The current checkout also
requires migration 11 for worker assignments and migration 10 for hosted sign-in
state; migrate and reapply runtime
grants before starting either role. See [hosted deployment](hosted-deployment.md).

Start the API in one terminal:

```sh
AYATI_WORKER_PROVIDERS=fireworks-ai AYATI_CHAT_EXECUTION=external cargo run --locked -- identity
```

Start a worker in another terminal, with its own scratch directory:

```sh
AYATI_DATA_DIR=.ayati-local/chat-worker cargo run --locked -- worker
```

The API requires its existing WorkOS configuration. The worker requires the
PostgreSQL runtime connection, pinned Goose binary and credentials for the model
providers used by the selected agent profiles. It opens no HTTP listener, needs no
WorkOS credentials or Library object-store credentials, and never opens the API's
local agent registry or private VM-backed workspace.

Set `AYATI_WORKER_PROVIDERS` on the API to the providers actually configured on
workers (`openai`, `fireworks-ai`, or both, comma separated). This advertises model
choices without giving the API model keys; it is not a worker availability check.
The optional `AYATI_WORKER_HEALTH_FILE` and `ayati worker-health` support a process
supervisor; [the hosted guide](hosted-deployment.md) describes readiness and limits.

`AYATI_CHAT_EXECUTION=embedded` is the default for existing installations. It keeps
the original in-process shared dispatcher. `external` disables that dispatcher in
the API while retaining messaging/event delivery. Unknown modes and external mode
without PostgreSQL fail startup. An explicitly started worker consumes PostgreSQL
jobs regardless of the API's mode, so configure the API as external when checking
the split. Multiple consumers use the same transactional claims.

`AYATI_WORKER_CONCURRENCY` defaults to 4, accepts 1 through 16, and limits the
independent process's active assignments. Workers poll eligible jobs every 500 ms;
an in-process notification is not needed for cross-process delivery. Aggregate
workspace/fleet fairness remains future work.

## Request and authority contract

1. The authenticated API validates and commits the message plus queued reply/job.
2. A worker reserves local capacity, then claims a job with PostgreSQL's existing
   transaction, unique claim token and five-minute lease.
3. The worker rechecks membership, agent grants, pause state, conversation and
   Library source access; it loads a permitted PostgreSQL agent profile.
4. A fresh Goose session receives bounded conversation context, configured
   instructions, permitted notes and explicit source versions. It has no business
   tools, VM, browser, private memory tools or default Goose extensions.
5. Before publication PostgreSQL rechecks the current claim and access in the
   existing workspace transaction. Old claims cannot replace a newer result.
6. The API serves the saved result through the existing message history/event path.

The worker deliberately overrides inherited local VM/browser/memory/proof-tool
settings. Goose inherits only its existing allowlisted process environment, not
the database or WorkOS credentials. This is a conversation-only execution boundary;
it is not proof of OS sandboxing against arbitrary generated code or a completed
remote tool gateway. Legacy private-runtime jobs require the embedded path.

Each process and attempt has a separate private scratch directory. Temporary
SQLite/Goose files hold only that attempt's runtime state; PostgreSQL owns accepted
requests and published results. Graceful completion removes temporary attempt data,
including source excerpts, instead of reusing a session across requests.

## Shutdown and failure

SIGINT and SIGTERM stop new claims and cancel active conversation generation.
Each task stops its Goose runtime before expiring its exact PostgreSQL claim;
another worker can immediately recover it. Claim release is conditional, so a stale
worker cannot expire the replacement's claim. Existing API/simulation shutdown now
also recognizes SIGTERM.

Abrupt loss leaves the claim to expire. The existing three-attempt limit applies;
the next claim pass visibly fails an exhausted reply. Graceful release retains the
running state so the final attempt is not stranded in an ineligible queued state.
Conversation generation can repeat after loss; only one accepted terminal reply is
published. This does not promise exactly-once model calls or external actions.

If the result cannot be saved, the worker leaves recovery to lease expiry rather
than acknowledging a missing result. Access changes cancel publication. The
180-second conversation deadline remains below the five-minute lease; there is no
lease heartbeat or arbitrary long-running workflow support in this increment.

Force-killed processes can leave private scratch files. A replacement uses a new
directory and never reuses that state. Controlled orphan retention/cleanup and
ephemeral container-volume behavior must be verified before hosted use. The worker
is trusted platform code with a runtime DB role; it is not an untrusted sandbox.

## Verification

```sh
cargo build --locked
cargo test --locked --lib
# Set RELAY_TEST_DATABASE_URL to a disposable PostgreSQL database first:
cargo test --locked --lib identity::postgres -- --ignored
```

The process test needs a separate local database whose name starts with
`relay_simulation_`, an administrator fixture connection in
`RELAY_TEST_DATABASE_URL`, `psql`, a prepared `relay_app` role, and pinned Goose.
It creates a unique schema, migrates only that schema and runs the API and workers
as the restricted runtime role. It never targets ordinary identity data.

```sh
node --test tests/independent_chat_worker.mjs
```

Recorded checks: 67 default Rust tests and 39 real PostgreSQL tests pass, including
claim release, stale-worker fencing, exhausted retries and revocation. The process
journey uses the real pinned Goose process with a loopback scripted model provider.
It checks no-worker queueing, API restart, worker restart, abrupt loss, competing
workers, conversation/source isolation, revocation and graceful scratch cleanup.
The crash test explicitly expires only its fixture lease instead of waiting five
minutes. Test reports/process logs are retained under `.tmp/independent-worker-*`;
crash scratch is retained separately for reconciliation.

No paid inference, external business actions, real WorkOS calls or cloud resources
are used by these process tests. Hosted HTTPS/login-flow state, remote tools,
distributed budgets, fleet scheduling, general source retention/cleanup and
production recovery remain separate acceptance work.

# Local controller-scoped assignment execution

M1.5a adds an opt-in machine API to the Rust controller. A local launcher registers
an executor, reserves an assignment and sends its child a scoped capability over
stdin. The child loads controller-prepared context, runs pinned tool-free Goose and
renews/completes through HTTP. It receives no core PostgreSQL URL, WorkOS key or
operator bootstrap key. The controller owns all PostgreSQL access.

Each assignment still creates a fresh worker/Goose process. Ready shared slots,
warm-harness latency, token streaming, checkpoints and AWS machine identity remain
later increments. The [direct-PG M1 path](assignment-workers.md) remains available.

## Migration and local startup

Run all twelve migrations, including `m20261023_000012_worker_control`, using the
migration owner and reapply runtime grants. Startup verifies schema and never
migrates ordinary databases. Migration 12 adds executor credentials and assignment
scope/context/completion records while preserving existing jobs and M1 assignments.
See [PostgreSQL setup](postgres-workspaces.md).

Configure these on the controller and local launcher:

~~~sh
install -d -m 700 .ayati-local
export AYATI_CHAT_EXECUTION=assigned
export AYATI_WORKER_CONTROL_URL=http://127.0.0.1:8766
export AYATI_WORKER_CONTROL_KEY_FILE="$PWD/.ayati-local/worker-control.key"
~~~

The key path must be absolute, its parent private, and the file a regular file owned
by the current OS account with mode 0600. Symlinks are refused. Create it once
without printing or overwriting an existing key:

~~~sh
python3 - <<'PY'
import os, secrets
fd = os.open(os.environ['AYATI_WORKER_CONTROL_KEY_FILE'],
             os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
with os.fdopen(fd, 'w') as output:
    output.write(secrets.token_urlsafe(32) + '\n')
PY
~~~

Start `ayati identity` with the private database configuration and advertised worker
providers. In another terminal, start the launcher with its own data directory,
pinned Goose binary and supported provider/model setup:

~~~sh
AYATI_RUNTIME=goose AYATI_DATA_DIR=.ayati-local/control-launcher \
  cargo run --locked -- assignment-launcher
~~~

With `AYATI_WORKER_CONTROL_URL`, the launcher uses the machine API and needs no
database connection. Without it, legacy configuration uses direct PostgreSQL.
Invalid control configuration fails instead of falling back. Both controller
settings must be present together. The launcher holds bootstrap access; its child
never receives that file path/value. Normal dotenv discovery still applies to the
operator launcher; use a private empty `.env`/explicit environment when separating
operator credentials. Child directories have their own empty `.env` fence.

The machine listener accepts explicit loopback IPs only. It refuses non-loopback
binds, credentials in URLs, redirects and proxy settings. Browser Origin/Cookie
headers, wrong Host headers and query strings are rejected. Browser sessions cannot
authenticate it. This same-host operator trust model is not AWS authentication or
transport suitable for an exposed network.

## Machine protocol

| POST route | Required credential | Responsibility |
| --- | --- | --- |
| `/v1/executors/register` | Operator bootstrap | Register exact executor/incarnation and hashed executor token |
| `/v1/executors/{executor}/{incarnation}/reserve` | That executor | Idempotent reservation and assignment credential binding |
| `/v1/executors/{executor}/{incarnation}/release` | That executor | Reconcile only its stopped child/failed launch |
| `/v1/assignments/{assignment}/start` | Assignment | Authorize and return prepared context |
| `/v1/assignments/{assignment}/renew` | Assignment | Recheck current authority/ownership and renew |
| `/v1/assignments/{assignment}/complete` | Assignment | Atomically save outcome and replayable receipt |

Credentials are random 256-bit tokens; PostgreSQL stores SHA-256 hashes. An uncertain
reservation retries its original request ID/token. A late replay returns only scope
metadata and never revives an expired capability. Executor credentials expire after
24 hours; launcher registration refreshes every five minutes and active renewals
also extend expiry. The bootstrap file is reread on registration refresh.

Scope binds executor, incarnation, slot identity, assignment, workspace, agent,
conversation, thread, reply and generation. Every current assignment gets a distinct
slot identity; reusable slot pools are not implemented. A worker chooses one process
session nonce. The same start nonce replays the original prepared context; a second
nonce is denied. Start confirms a fresh lease before acknowledging, including retries.

Prepared context separates trusted instructions, accepted request text, scoped notes
and authorized Library excerpts/provenance. Provider/model and accepted duration are
controller-selected. Context contains no operation grants/provider credentials.
Assembly, renewal and publication recheck current requester, agent, conversation
and source access. Publication and completion receipt commit together using existing
workspace/assignment fencing. Changed completion payloads conflict.

Executor/bootstrap credentials cannot authorize run routes. Assignment credentials
cannot register, reserve or access another run. UUIDs and PIDs are identities rather
than authentication. The launcher passes a private JSON envelope to
`ayati worker --controller-assignment` on stdin, never in arguments or logs.

## Recovery and bounds

- Existing lease/run/capacity bounds are retained; controller settings govern API
  reservations. This slice rechecks/renews every 500 ms for responsive cancellation.
  The worker keeps a conservative monotonic deadline including request time and a
  one-second clock-rounding margin. HTTP waits cannot extend that safe deadline.
- A transient controller failure may retry inside the confirmed lease window.
  Expiry or denial stops/reaps Goose. A longer outage permits a new tool-free
  attempt after fencing; it does not resume arbitrary in-memory state.
- Run capabilities expire after five minutes and extend only with valid renewal.
  Cancellation/release/expired-owner reconciliation invalidates them and clears
  prepared context. Completion clears context and retains a replayable receipt for
  five minutes. Durable executor/attempt/receipt history has no automatic deletion
  in this slice; a later explicit retention policy must cover it.
- Controller restart uses durable registrations, scope and receipts. The worker
  retries the same completion payload without repeating its model run. Unconfirmed
  completion exits as a failure for durable reconciliation. Tool-free recovery may
  repeat inference/cost; general external business effects remain disabled.
- Failure before an acknowledged start uses the existing bounded startup-recovery
  policy (at most three attempts). Each attempt rechecks context access; denied
  context never reaches Goose. A saved execution failure remains terminal.
- Context is bounded at 256 KiB, result text at 64 KiB, and machine requests/client
  responses at 512 KiB. Oversized generated output saves a terminal failure.
  Requests time out after two seconds. No SQL transaction spans model/network work.
- Local launcher loss retains M1's parent-death boundary. Stop/reap precedes resource
  release. User cancellation uses the existing authenticated conversation route.

Responses use 401 for missing/malformed credentials, 403 for denied credentials or
scope, 409 for state/payload conflict, 422 for schema errors, and 503 for database
unavailability. Errors omit underlying bodies/secrets. Machine responses are no-store.

Workers retain private scratch and a tool-free execution profile that overrides
inherited VM/browser/memory/tool settings. Model keys still reach the configured
Goose provider; per-run model brokerage remains future work. Process/private-scratch
isolation is not a microVM boundary.

## Verification

~~~sh
cargo test --locked --lib
cargo test --locked --lib identity::postgres -- --ignored
cargo clippy --locked --all-targets -- -D warnings
./scripts/check-file-lines.sh
cargo build --locked --bin ayati
node --test --test-concurrency=1 tests/worker_control.mjs
node --test --test-concurrency=1 tests/assignment_workers.mjs tests/independent_chat_worker.mjs tests/hosted_deployment.mjs
~~~

Set `RELAY_TEST_DATABASE_URL` privately. Process fixtures require a dedicated local
`relay_simulation_*` database, restricted runtime role and real pinned Goose with a
loopback scripted provider. They retain schemas, reports and process logs. Journeys
cover scope denial, expired/cancelled/stale credentials, environment isolation,
controller restart/lease loss and actually dropped start/completion responses.
Fixtures make no paid model or AWS calls. Local success does not establish live
provider quality, cloud identity, shared-slot readiness or warm response latency.

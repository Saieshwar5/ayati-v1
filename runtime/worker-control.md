# Worker control: controller ↔ pooled executors

The controller (`ayati identity`) is the only process with PostgreSQL, WorkOS and
connector access. Executors (`ayati executor`) run prepared Goose sessions and
reach the controller only through this machine API. They never receive a core
database URL, WorkOS key or connector credential; model provider keys stay on
executors. See the [executor pool](executor-pool.md) for slots and dispatch, and
[live replies](reply-streaming.md) for streamed output.

Every agent reply runs this way. The older embedded, polling-worker and
temporary-assignment modes were removed on 2026-10-29. `AYATI_CHAT_EXECUTION`
accepts only `pooled` (the default); `ayati simulate` alone may use `scripted`
(its default) to answer with deterministic replies inside the controller.

## Migration and local startup

Run all current migrations with the migration owner and reapply runtime grants
before starting (see [PostgreSQL setup](postgres-workspaces.md)). Startup verifies
the schema and never migrates it.

Configure the controller and every executor with the same loopback origin and key:

~~~sh
install -d -m 700 .ayati-local
export AYATI_WORKER_CONTROL_URL=http://127.0.0.1:8766
export AYATI_WORKER_CONTROL_KEY_FILE="$PWD/.ayati-local/worker-control.key"
~~~

The key path must be absolute and the file a regular file owned by the process's
own user with mode 0600. Symlinks are refused. Create it once without printing or
overwriting an existing key:

~~~sh
python3 - <<'PY'
import os, secrets
fd = os.open(os.environ['AYATI_WORKER_CONTROL_KEY_FILE'],
             os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
with os.fdopen(fd, 'w') as output:
    output.write(secrets.token_urlsafe(32) + '\n')
PY
~~~

Then start the controller and one or more executors; the
[executor pool guide](executor-pool.md) lists their exact environments. A
PostgreSQL controller refuses to start without this listener. In containers the
executor shares the controller's network namespace, and a one-shot helper hands
the key to the runtime UID (see `deploy/compose.yaml`).

The listener accepts explicit loopback IPs only. It refuses non-loopback binds,
credentials in URLs, redirects and proxy settings. Browser Origin/Cookie headers,
wrong Host headers and query strings are rejected; browser sessions cannot
authenticate it. This same-host trust model is not AWS authentication. The
optional [signed transport adapter](../integrations/aws_bridge/README.md) carries
it between hosts.

## Machine protocol

| POST route | Required credential | Responsibility |
| --- | --- | --- |
| `/v1/executors/register` | Operator bootstrap key | Register an exact executor/incarnation and hashed executor token |
| `/v1/executors/register-cloud` | Signed cloud identity | Bind a registration to one ECS task |
| `/v1/executors/{executor}/{incarnation}/pool/{events,capacity,drain,stop}` | That executor | Report prepared slots, receive offers, drain and stop |
| `/v1/assignments/{assignment}/start` | Assignment | Authorize and return controller-prepared context |
| `/v1/assignments/{assignment}/renew` | Assignment | Recheck current authority/ownership and renew |
| `/v1/assignments/{assignment}/output` | Assignment | Append bounded streamed reply output |
| `/v1/assignments/{assignment}/tools` | Assignment | Forward one scoped controller tool call |
| `/v1/assignments/{assignment}/complete` | Assignment | Atomically save the outcome and a replayable receipt |

Credentials are random 256-bit tokens; PostgreSQL stores SHA-256 hashes. Executor
credentials expire after 24 hours; registration refreshes and active renewals extend
them. Scope binds executor, incarnation, slot and slot generation, assignment,
workspace, agent, conversation, thread, reply and generation. A worker chooses one
session nonce: the same nonce replays the original prepared context and a second
nonce is denied. Start confirms a fresh lease before acknowledging, including retries.

Prepared context separates trusted instructions, accepted request text, scoped notes
and authorized Library excerpts. Provider/model and duration are controller-selected.
Context contains no operation grants or provider credentials. Assembly, renewal and
publication recheck current requester, agent, conversation and source access.
Changed completion payloads conflict. Executor credentials cannot authorize run
routes; assignment credentials cannot register or access another run.

## Recovery and bounds

- Renewal rechecks every 500 ms for responsive cancellation. Expiry or denial
  stops and reaps Goose; a newer authorized attempt never resumes in-memory state.
- Run capabilities expire after five minutes and extend only with valid renewal.
  Cancellation, slot replacement and expired-owner reconciliation clear prepared
  context. Completion keeps a replayable receipt for up to five minutes.
- Controller restart uses durable registrations, scope and receipts. Executors retry
  the same completion payload without repeating the model run.
- Failure before an acknowledged start uses bounded startup recovery (at most three
  attempts). Denied context never reaches Goose.
- Context is bounded at 256 KiB, result text at 64 KiB and machine requests at
  512 KiB. No SQL transaction spans model or network work.

Responses use 401 for missing/malformed credentials, 403 for denied credentials or
scope, 409 for state/payload conflict, 422 for schema errors and 503 for database
unavailability. Errors omit underlying bodies and secrets; responses are no-store.

Each Goose run keeps a private per-run journal (`src/journal`) in its scratch
directory: one request, its claim, the streamed reply, tool observations and
approvals. It is deleted after the run and never shared.

## Verification

~~~sh
cargo test --locked --lib
./tools/local-postgres.sh test
node --test --test-concurrency=1 tests/executor_pool.mjs tests/hosted_deployment.mjs
python3 tools/run-agent-load.py --suite regressions
~~~

Process fixtures need a dedicated `relay_simulation_*` database, the restricted
runtime role and real pinned Goose with a loopback scripted provider. They make no
paid model or AWS calls and do not establish provider quality or cloud latency.

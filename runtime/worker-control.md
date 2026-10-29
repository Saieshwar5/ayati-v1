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
PostgreSQL controller refuses to start without this listener.

## Executors on other machines (private network, TLS)

Executors are an independent fleet: any number reach a controller over a private
network. Worker control then serves TLS signed by an operator-owned private
certificate authority, and executors trust only that authority:

~~~sh
tools/worker-control-certs.sh /absolute/private/worker-control-tls worker-control
~~~

| Process | Settings |
| --- | --- |
| Controller | `AYATI_WORKER_CONTROL_URL=https://worker-control:8766` (the name executors use; checked against each request's Host header), `AYATI_WORKER_CONTROL_LISTEN=0.0.0.0:8766`, `AYATI_WORKER_CONTROL_TLS_CERT=…/server.pem`, `AYATI_WORKER_CONTROL_TLS_KEY=…/server.key` (0600, owned by the process user) |
| Executor | The same URL, `AYATI_WORKER_CONTROL_CA=…/ca.pem`, and the bootstrap key file |

Rules, enforced at startup: TLS is used exactly when the URL is `https://`; plain
HTTP is allowed only on a loopback IP; only TLS may listen beyond loopback; an
`https://` executor requires the CA and refuses public roots, plain HTTP and
redirects; executors refuse the controller's TLS key, certificate and listen
address, and controllers refuse the CA setting. TLS handshakes run off the accept
path, so a silent client cannot stall others. The tool deletes the CA private key;
issue a new set to rotate.

Valid TLS grants nothing by itself: every route still requires the bootstrap key,
an executor token or a per-assignment capability. Browser Origin/Cookie headers,
wrong Host headers and query strings are rejected; browser sessions cannot
authenticate it. Network policy (Compose `internal` network, AWS security groups)
should still admit only executors.

## Executors on AWS: proven task identity

With `AYATI_EXECUTOR_IDENTITY=aws` an executor needs no bootstrap key. When it
registers (and again every five minutes) it signs an STS `GetCallerIdentity`
request with its ECS task-role credentials, covering a digest of its exact
registration in the signed `x-ayati-registration` header, and sends the signed
headers to `POST /v1/executors/register-aws` instead of sending them to AWS.

The controller (with `AYATI_CLOUD_WORKERS=1` and the cluster, release and role
ARNs) checks the headers' shape and binding, sends that exact request to the
regional STS endpoint itself, and takes the caller ARN from AWS's answer. It then
calls ECS `DescribeTasks` and `DescribeTaskDefinition` with its own credentials:
the task must be in the configured cluster, `RUNNING`, on the configured task
definition, with the configured task role. The registration is bound to that task
(migration 17) and the executor sends its task ARN with every later request.

Requirements and limits:

- The executor task role's trust policy must allow only `ecs-tasks.amazonaws.com`.
  The controller derives the task from the role session name; ECS is expected to
  name task-role sessions after the task ID. Confirm this in the first AWS window.
  Any other session name is refused.
- The controller's task role needs `ecs:DescribeTasks` and `ecs:DescribeTaskDefinition`.
  STS `GetCallerIdentity` needs no permission.
- STS refuses signatures older than 15 minutes; a replay inside that window only
  repeats the identical registration.
- `AYATI_AWS_TEST_ENDPOINT` points STS and ECS at a loopback fake for local tests
  only; any other value is refused.

The optional [signed transport adapter](../integrations/aws_bridge/README.md) is the
earlier gateway-based design for this hop; it remains until the native path is
proven on AWS.

In `deploy/compose.yaml` executors are their own service on an internal `worker`
network with model egress and no database route; scale them with
`--scale executor=N`. A one-shot helper installs the bootstrap key, TLS key and
certificate for the API and the key and CA for executors into separate volumes,
because both run as UID 10001.

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

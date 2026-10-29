# Tests

## Temporary assignment workers

The backend [assignment worker contract](../runtime/assignment-workers.md) describes
the separate real-Goose/PG process rehearsal. With a dedicated local
RELAY_TEST_DATABASE_URL whose database starts with relay_simulation_:

~~~sh
cargo build --locked --bin ayati
node --test --test-concurrency=1 tests/assignment_workers.mjs
~~~

It checks startup from zero workers, one-assignment exit, ownership renewal,
competing launchers, cancellation/revocation and process-loss recovery. Set
AYATI_ASSIGNMENT_SOAK_SECONDS=185 to cover the former three-minute ACP limit.
The provider is scripted and all database records are disposable. This does not
test AWS, paid models, browser tools or business-effect checkpoint continuation.

## Prepared shared executors

The [executor pool contract](../runtime/executor-pool.md) describes M1.5b. With the
same dedicated disposable database and real pinned Goose, run:

~~~sh
node --test --test-concurrency=1 tests/executor_pool.mjs
~~~

It checks prepared PID dispatch, concurrent sessions and independent supervisors,
workspace isolation, capacity queueing, session replacement, controller restart,
executor crash, graceful drain, failed readiness and cancellation. The provider is
scripted; no cloud or paid model is used. Preserve an immutable `AYATI_TEST_BIN`
copy if Cargo will rebuild the binary while process tests are running.

## Generic preparation before readiness

Pooled fixtures require migration 15 and the separate prepared artifact:

~~~sh
python3 tools/build-prepared-goose.py
python3 tools/run-agent-load.py --suite preparation --binary /absolute/path/to/frozen/ayati
~~~

The [preparation contract](../runtime/harness-preparation.md) describes the tiny
pinned Goose patch. Its direct ACP probe compares the same ELF with/without
preparation across eight fresh processes and verifies zero inference before
readiness. Additional journeys refuse a matching-version process missing the
acknowledgment and drain blocked preparation children. PostgreSQL tests cover
legacy reports, version-14 migration, preserved owned work and atomic start replay.
The runner creates a fresh private database and verifies resource limits; use
`--executor-cpu 200` for the two-CPU profile. Raw failures and successes are retained.

## Exact release-image checks

With Docker Engine/Buildx, systemd user scopes and local PostgreSQL tools:

~~~sh
python3 tools/verify-release-image.py
~~~

The [image guide](../runtime/release-image-validation.md) documents the private
fixture data, exact image hashes, resource guards and cleanup ownership. No host
application/Goose build is needed. The complete workflow passes thirteen process
journeys: image setup/readiness/assets, mixed chat/background work, recovery and
two 384-request load matrices. Every service runs the same immutable image;
executors have private PID namespaces and actual container cgroup limits.
`--smoke-only` omits the ordinary matrices. `--image sha256:...` tests an existing
immutable local image. This is scripted-provider, same-host evidence.

Nine Python guards cover resource-scope and container cleanup ownership:

~~~sh
python3 -m unittest discover -s tests -p 'agent_*_test.py'
python3 -O -m unittest discover -s tests -p 'agent_*_test.py'
~~~

## Signed controller/worker transport

The [adapter guide](../integrations/aws_bridge/README.md) describes its isolated
dependencies and local TLS/SigV4 harness. Run Python boundary/stream checks with:

~~~sh
python -m unittest discover -s tests -p 'aws_bridge*_test.py'
~~~

After a native build, use a dedicated local
`RELAY_TEST_DATABASE_URL` and the locked environment's interpreter:

~~~sh
export AYATI_TEST_BRIDGE_PYTHON="$PWD/.tmp/aws-bridge-tools/bin/python"
unset AYATI_TEST_PREPARED_GOOSE_BIN
node --test --test-concurrency=1 tests/aws_worker_bridge.mjs
~~~

This rehearses real Rust execution with pinned prepared Goose through a local TLS
gateway, including five employee/agent identities, task-binding denial, controller
restart, Stop and worker loss. Build/verify prepared Goose first. An explicit
`bridge_runtime.mjs` override remains a protocol-only test and cannot satisfy
runtime acceptance.
No AWS or paid provider is involved. Ordinary database
migration is never implied; startup requires migration 17 and current grants.

The [packaged bridge](../runtime/packaged-worker-bridge.md) adds an isolated
`python3 tools/run-agent-load.py --suite bridge --executor-cpu 200` journey and
the same real prepared-Goose path in `tools/verify-release-image.py`, including
`--smoke-only`.

## Provider calibration

The [calibration guide](../runtime/provider-calibration.md) describes the explicit
live opt-in, fixed upstream endpoints, token/request/concurrency limits, private
credentials and conservative cost accounting. Run the boundary tests without
Docker or paid calls:

~~~sh
node --test tests/provider_calibration_guards.mjs
~~~

With the verified immutable image, Docker and Playwright Chromium:

~~~sh
node tools/calibrate-provider.mjs --image sha256:...
~~~

The default scripted rehearsal runs actual image roles and Chat browser sends,
measures five healthy foreground replies across individual/concurrent/mixed work,
and checks background completion and Stop. Thirteen boundary checks cover live
opt-in, key privacy, atomic/nonrefundable accounting, token limits, SSE chunking,
errors, redirects, missing usage, cancellation and allowance exhaustion.
The [first live Fireworks profile](../runtime/provider-calibration.md#first-live-fireworks-profile--2026-10-24)
now passes seven requests with real browser timing and Stop. This small local
sample does not establish cloud latency or general provider/model acceptance.

## Durable live replies and local timing

For [reserved chat/background admission](../runtime/interactive-reserve.md), build
and freeze a binary after migration 16, then run:

~~~sh
python3 tools/run-agent-load.py --suite mixed --executor-cpu 200 --binary /absolute/path/to/frozen/ayati
~~~

It measures 96 short small/large-context requests beside held background calls
and a queued backlog, then checks background completion. Other journeys verify
authenticated immutable intake/backpressure, a smaller actual fleet, graceful
drain, revoked grants and controller/executor recovery. Fixtures use the real
background API rather than relabeling database jobs. Timings cover ready capacity;
replenishment and overflow are distinct. The existing 64 total/32 background
pending-request bounds are tested independently from execution reserves.

See the [streaming contract](../runtime/reply-streaming.md). After building Rust
and web assets, use the same private disposable database to run:

~~~sh
node --test --test-concurrency=1 tests/reply_stream.mjs tests/reply_capacity.mjs
~~~

These use actual Goose and a scripted model to exercise partial text, browser
send/offline/reload/Stop, controller replacement, dropped output acknowledgments,
interrupted attempts and bounded-output failures. The named 24-sample capacity
journey records raw timings/quantiles and resource observations, then rehearses
held/short work, saturation, delayed executor growth and draining. Proposed latency
target failures remain in its report. Screenshots and JSON stay in retained fixture
directories; this is not a live-provider, cloud or multi-hour benchmark.

## Controlled concurrent load and recovery

See the [load rehearsal](../runtime/load-acceptance.md). With the same dedicated
local database and an immutable `AYATI_TEST_BIN`:

~~~sh
node --test --test-concurrency=1 tests/agent_load.mjs tests/agent_load_faults.mjs
~~~

The default matrix sends 384 measured requests: two repetitions, small/large
scoped contexts, concurrency 1/4/8/16 and four prepared slots. It pins the driver,
controller and executor processes to four CPUs; this is affinity, not a quota or
exclusive host. It records phase traces, prompt sizes, p50/p95/p99, resource
samples and latency-target misses. Separate fault journeys run controller restart
with backlog, real executor lease expiry, and cancellation/revocation beside other
active requests. Reports and failed runs remain in printed fixture directories.
The model is scripted and PostgreSQL remains outside that affinity; these are not
cloud sizing or live-provider performance results.

For the follow-up with fresh PostgreSQL and verified CPU/memory limits, use:

~~~sh
python tools/run-agent-load.py
python tools/run-agent-load.py --executor-cpu 200
python tools/run-agent-load.py --suite faults
python tools/run-agent-load.py --suite regressions
~~~

The [bounded rehearsal guide](../runtime/bounded-agent-load.md) describes required
local tools, resource ceilings, timestamp precision and retained evidence. Each
invocation owns and stops its test cluster/scopes and keeps their files; no
existing database is reconfigured. These commands require a working systemd user
manager and cgroup v2, and fail instead of falling back to unbounded execution.

## Shared workspace journeys

`RELAY_TEST_DATABASE_URL=... node --test tests/agents_team_workspace.mjs` verifies
the selected Agents team/profile layout, original channel/thread focus beyond
the latest history page, saved progress without model work, exact queued/running/
waiting Stop, private work/count isolation, invalidated approval denial, restart
history, and mobile/dark layouts. It uses a disposable PostgreSQL schema, pinned
Goose, a scripted model and an authenticated MCP fixture; no paid inference or
cloud is needed. See [the request API and limits](../runtime/agents-team-workspace.md).

`RELAY_TEST_DATABASE_URL=... node --test tests/agent_questions.mjs` verifies real
Goose asks for ordinary typed facts, releases its worker, resumes the original
channel/thread after saved answers and controller restart, then obtains separate
exact business approval before one write/verification. It covers all form fields,
closed and multiple choices, enabled alternatives, managed private agents, No /
close / decline / Stop, current source access, keyboard and phone/dark UI. A
second case exercises the controller-local broker in an isolated compatibility
worker; the embedded simulation API intentionally remains deterministic. Both
use disposable schema 23 and scripted providers, with no cloud or paid inference.
See [contracts, migration and adapter limits](../runtime/agent-questions.md).

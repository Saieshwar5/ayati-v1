# Controlled local executor load and recovery

This increment measures the existing tool-free pooled chat path under concurrent
work. It adds opt-in startup traces and repeatable load/fault journeys, and fixes
a PostgreSQL deadlock discovered by four concurrent requests. It does not deploy
AWS, change the selected harness, enable tools or establish a production SLA.

The follow-up [bounded local rehearsal](bounded-agent-load.md) adds a fresh private
PostgreSQL cluster per invocation, verified CPU/memory ceilings and timestamps
covering offer delivery through actual fixture model HTTP input. Historical
observations below retain their original affinity-only conditions.

## The concurrency fix

Workspace writers previously took `FOR UPDATE` on the workspace row. Concurrent
intake and assignment projection transactions could acquire locks in this cycle:
workspace row, message sync clock, workspace foreign-key check. A dispatch insert
could hold the sync clock while its foreign-key `KEY SHARE` waited on a workspace
writer that was itself waiting on the sync clock. Intake returned HTTP 503.

Writers now use `FOR NO KEY UPDATE`: workspace writers still serialize and
authorization readers using `FOR SHARE` still wait for the current decision.
Foreign-key `KEY SHARE` can coexist because these operations do not change the
workspace key. No permission cache or blind write retry was added. A regression
holds an uncommitted agent pause, allows a metadata-only offer, proves context
start waits, commits the pause and verifies start is denied without saving context.
All PostgreSQL regressions cover the wider workspace lock change.

There is no schema change; use migration 14 and the existing restricted runtime
grants from [PostgreSQL setup](postgres-workspaces.md). Ordinary databases were
not migrated during this work.

## Removing per-run transport initialization

The first successful matrix also exposed work before the controller request:
every assignment constructed a new `reqwest::Client` synchronously in the slot
supervisor. A focused local check of the same builder measured 54–72 ms per
construction. That cost was repeated for each offered run, delaying concurrent
handoff despite prepared Goose sessions.

Run clients now clone the supervisor's HTTP transport and bind their own validated
capability. The transport has no default Authorization header or cookie store;
each request sets its own bearer credential. The fixed validated origin, proxy/
redirect restrictions and request timeout remain. Concurrent loopback regression
requests verify that two run credentials and the supervisor credential stay
separate across repeated connection use. This shares neither context nor authority.
Cold compatibility workers retain their existing transport initialization.

## Running the workload

Build the binary and copy it to a private immutable path before starting process
fixtures. Set `AYATI_TEST_BIN` to that copy and privately set
`RELAY_TEST_DATABASE_URL` to a dedicated local `relay_simulation_*` database.
Fixtures create their own schemas and retain synthetic records for inspection.
Use the existing real pinned Goose 1.54.0 installation.

~~~sh
cargo build --locked --bin ayati
node --test --test-concurrency=1 tests/agent_load.mjs tests/agent_load_faults.mjs
~~~

The default matrix is two repetitions of all eight context/concurrency cases:

| Parameter | Named profile |
| --- | --- |
| Controller/executors | One controller, two executors, two prepared slots each |
| Capacity | Four running slots; workspace/model caps 32; one active run per agent |
| Concurrency | 1 and 4 within reserve; 8 and 16 overflow the same reserve |
| Samples | 24 per case per repetition, 384 measured requests in total |
| Small context | Fresh group, unique request marker and 256-character body, no knowledge |
| Larger context | Fresh group, four human history messages, an 8,000-character body and an approved agent-scoped knowledge note |
| Provider | Loopback script, actual Goose, 30 ms delay before first public token |
| Resources | Four-CPU affinity for driver/controller/executors/Goose; no CPU or RAM quota |
| Lease | Twelve seconds, with normal renewals |

`AYATI_LOAD_CONCURRENCY` selects comma-separated values from 1/4/8/16;
`AYATI_LOAD_CONTEXTS` selects small/large; `AYATI_LOAD_SAMPLES` is 4–40,
`AYATI_LOAD_REPEATS` 1–5 and `AYATI_LOAD_CORES` 2–16. Smaller smoke runs are useful
for correctness but do not replace the named repeated profile. Fault journeys
use four-CPU affinity independently of those matrix overrides.

Every wave begins with four compatible ready slots. Healthy requests must have
one provider invocation and one attempt, correct final state, only their own
conversation marker and exactly the expected approved agent knowledge. Each
request has a fresh group to avoid direct-conversation deduplication or history
growth changing the workload. Seeding and waiting for replenishment are outside
accepted-to-final timing; replenishment affects overflow requests within a wave.
The last wave may be smaller: 24 samples at concurrency 16 means waves of 16 and 8.

The observer uses asynchronous batched SQL. The old synchronous fixture helper
blocked the Node event loop that also hosted the model fixture; that would distort
concurrent model-request timings. Legacy tests retain the old helper, while this
matrix records driver event-loop delay and includes observer work in its resource
scope. Do not compare the older single-message rehearsal as an equivalent load.

## What the report measures

Each fixture directory retains `load-report.json`, `report.json` and complete
`process-*.log` files. The load report stores configuration, environment, immutable
binary path, raw per-request timings, prompt byte counts, traces, resources and
failure/target flags. It is saved before and between cases and on failure.
Failed intake is an error, not an automatically retried sample.

| Metric | Boundary |
| --- | --- |
| Client acknowledgment | Local client send to HTTP acknowledgment |
| Dispatch | Durable job creation to assignment reservation |
| Context | Durable creation to controller context/start commit |
| Fixture model request | Durable creation to actual loopback provider HTTP input |
| Fixture first token | Durable creation to first public scripted provider token |
| Saved text | Durable creation to first committed controller draft |
| Final | Durable creation to final committed message |

Distributions use nearest-rank p50/p95/p99; with 24 samples p99 is the maximum.
Both repetitions are retained independently. Proposed p95 targets remain below
100 ms dispatch and below 250 ms model request for compatible ready capacity.
Overflow cases keep separate distributions and are excluded from the combined
ready-capacity target flag. Target misses stay in the report without disguising
them as correctness failures or dropping samples.

On controller and executor, `AYATI_TRACE_START=1` with `RUST_LOG=ayati=info` emits
`ayati::performance` start logs correlated by assignment ID and side. The JSON
contains version, total microseconds and fixed-label phase durations:

- Controller: scope lookup, profile lookup, authority before/after preparation,
  Library sources, settings, instructions/notes, encoding, start commit and lease
  confirmation. An already saved context replay omits preparation phases.
- Executor: credential binding, controller round trip, local scratch task/reply setup, prompt-event
  observation and waiting-report acknowledgment.

The flag is off by default. Traces contain durations and assignment IDs, never
context bodies, prompts, source content or credentials. They trace successful
starts, not a complete error profiler. The prompt event indicates ACP submission;
it is not the provider HTTP boundary. The waiting-report path and model request
can run concurrently. Do not add these durations as if every operation were serial
or infer the uninstrumented Goose/provider gap to be a specific upstream bug.

Resource reporting samples driver/controller/executor families every 200 ms:
deduplicated live RSS, per-role sampled RSS maxima, parent CPU including waited
children, and driver event-loop delay. CPU excludes still-live child CPU and
PostgreSQL. RSS is neither a hard limit nor an allocation recommendation. Shared
host contention, PostgreSQL resources and short peaks between samples remain
uncontrolled. CPU affinity is neither exclusive cores nor a cgroup quota.

## Faults while other work is active

`tests/agent_load_faults.mjs` retains `load-fault-report.json` for each scenario:

- Restart the controller with four held runs and four queued requests. All claims
  survive; no new inference is caused by restart; eight requests finish once.
- Kill one executor with two partial replies beside two survivors. Wait for actual
  twelve-second leases to expire; only lost runs retry with new assignments and
  generation two. Old draft text is retained privately and never joined to the
  replacement. Surviving ownership is unchanged; child processes are stopped.
- Cancel one partial reply and revoke another requester's agent grant while two
  authorized replies remain active. Denied drafts disappear from history/replay,
  their harnesses stop and no denied final text enters the output ledger.
  Authorized neighbors finish without restarting inference.

These are real local process failures, not accelerated expiry or cloud failover.
Recovery time includes configured lease expiry and replacement preparation.

## Retained results and remaining gate

The final named run is retained at `.tmp/independent-worker-vrK6KH`, using
immutable debug binary SHA-256 prefix `c25006faf0d2f064`. All 384 requests passed
scope, attempt, inference-count and final-state checks. Each cell below lists
repetition 1 / repetition 2 p95 milliseconds; overflow includes queueing:

| Context | Concurrent requests | Dispatch | Fixture model request | Saved text |
| --- | ---: | ---: | ---: | ---: |
| small | 1 | 16 / 14 | 270 / 259 | 324 / 311 |
| small | 4 | 64 / 55 | 595 / 593 | 651 / 653 |
| small | 8 | 1125 / 1055 | 1692 / 1582 | 1751 / 1658 |
| small | 16 | 3213 / 3308 | 3647 / 3711 | 3715 / 3779 |
| large | 1 | 21 / 15 | 281 / 274 | 333 / 328 |
| large | 4 | 53 / 53 | 592 / 598 | 649 / 652 |
| large | 8 | 1056 / 983 | 1654 / 1633 | 1707 / 1688 |
| large | 16 | 3207 / 3339 | 3807 / 3817 | 3867 / 3902 |

Ready-capacity dispatch passed the provisional 100 ms target. **The 250 ms model-
request target failed**, including one-agent cases; response-speed acceptance
remains open. Controller context p95 was 56–82 ms for one agent and 214–255 ms
for four. Actual prompt payloads were about 18.7 / 37.0 kB (decimal),
including Goose's own prompt, not just the test message body. Credential-binding
p95 was at most 38 microseconds across cases; this does not prove an end-to-end
latency win.

Before transport reuse, the complete 384-request matrix at
`.tmp/independent-worker-9FhEPD` passed correctness and measured one-agent p95
model request 185–225 ms and four-agent 530–576 ms. Keep both complete matrices:
the final run was slower overall, and this shared-host comparison does not
establish a consistent performance improvement. Do not select the fastest repeat
or attribute the difference to one code path without a controlled comparison.
The initial pre-lock-fix calibration failed with an actual deadlock at
`.tmp/independent-worker-N5evhp`; its evidence remains retained.

Final sampled maxima were 48.2 MiB controller, 453.3 / 464.6 MiB executor families
including Goose, and 222.0 MiB driver/model fixture. Simultaneous sampled total
peaked at 1,129.1 MiB. Independent role maxima need not occur together. The largest
case-level driver event-loop p95 was 23.3 ms. These debug/shared-host observations
are not container allocations or evidence that a 512 MiB production task is safe.

Final fault reports are retained at `.tmp/independent-worker-Ps1NnD`,
`.tmp/independent-worker-KNl9fU` and `.tmp/independent-worker-dBPBFD`. Restart to all
eight final replies took 1,714 ms; executor loss to replacement draft took
11,350 ms with the configured twelve-second renewable lease. All three scenarios
passed. These are observations, not recovery guarantees.

Verification includes 148 default Rust tests, 74 disposable PostgreSQL tests,
strict all-target Clippy, debug build, format/diff/500-line checks, the matrix and
three fault journeys, plus all twenty streaming/pool/capacity/compatibility
process journeys. Frontend source is unchanged; the existing real browser
stream/reconnect/Stop journey was executed rather than only type checked.
No AWS or paid-model calls were made.

Because the complete matrices ran at different times on a shared host/database,
a follow-up used four fresh disposable databases in before/after/after/before
order, each with sixteen small-context requests at concurrency four. All 64
requests passed. Model-request p95 was 940 / 600 / 597 / 722 ms respectively.
This narrower comparison supports transport reuse for that workload; it does
not erase the complete matrices, establish all-load improvement or pass 250 ms.
Raw reports, binary hashes, database names and fixture paths remain in
`.tmp/load-acceptance-verification/transport-comparison.json` and its order-specific
logs. Use a fresh dedicated database for future performance comparisons; retaining
many fixture schemas changes catalog/WAL/cache conditions even with isolated rows.

Next, narrow the remaining reservation-to-start and ACP-to-provider intervals,
measure repeated controller reads and contention, and repeat on a fresh dedicated
database with controlled PostgreSQL resources. Keep current authority checks.
Then perform a separately authorized bounded live-provider calibration before
M0 sizing and the M2/M3 AWS service/identity trial. Local scripted evidence alone
does not prove live-model latency, Fargate sizing, foreground/background
reservations, multi-hour operation or cloud machine identity.

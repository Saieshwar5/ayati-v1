# Bounded local agent latency rehearsal

This follows the [first load increment](load-acceptance.md). It adds fresh private
PostgreSQL clusters, verified resource ceilings and handoff timestamps so worker
CPU pressure can be distinguished from controller work and transport. These are
local debug-build/scripted-provider measurements, not cloud capacity or an SLA.

## Reproduce

Use Linux with a working systemd user manager, cgroup v2 CPU/memory controllers,
`taskset`, Node, `psql`, PostgreSQL server tools and the existing pinned Goose
1.54.0 installation. A missing prerequisite or failed limit check fails the run;
the bounded command does not silently substitute affinity-only execution.

~~~sh
cargo build --locked --bin ayati
python tools/run-agent-load.py
python tools/run-agent-load.py --executor-cpu 200
python tools/run-agent-load.py --suite faults
python tools/run-agent-load.py --suite regressions
python -m unittest discover -s tests -p agent_load_runner_test.py
~~~

For CPU/allocation comparisons, pass the same immutable binary path to both
invocations and compare their recorded SHA-256 hashes. Cargo test/build commands
can replace `target/debug/ayati` between invocations even with unchanged behavior.

`--binary /absolute/path/to/ayati` selects a before/after build. The runner copies
it to the evidence directory before launching anything. Matrix builds must emit
the new correlated start points; an older pre-point build fails that coverage
check. `--postgres-bin` or
`PG_BINDIR` selects server tools; the installed local bundle is also discovered.
`PG_SHAREDIR` overrides the PostgreSQL share directory. For a smoke run use
`--samples 8 --repeats 1 --concurrency 1,4 --contexts small,large`.

Each invocation creates its own `.tmp/bounded-agent-*` directory and PostgreSQL
cluster, private Unix socket and `relay_simulation_bounded_*` database. No TCP
database listener or password is required. The runtime role is unprivileged;
fixtures apply their existing restricted table grants. `fsync`, synchronous
commit and full-page writes must be on and are recorded. Schema/catalog history
from previous fixtures is absent at startup. Setup and seeding are outside
accepted-message timings.

The existing `.ayati-local` database is not reconfigured or stopped. On exit,
only the owned postmaster and recorded random-prefix test scopes are stopped.
Cluster files, fixture schemas, reports and failures remain for inspection.
Temporary user scopes are local test infrastructure; the cloud plan still uses
the ECS service and controller-owned admission.

## Resource profile

All processes use the first four allowed logical CPUs. Every role has its own
verified cgroup v2 ceiling:

| Role | CPU ceiling | Memory ceiling | Includes |
| --- | ---: | ---: | --- |
| Driver/model fixture | 1 CPU | 512 MiB | Node test runner, model HTTP server and SQL observers |
| Controller | 1 CPU | 512 MiB | Rust controller descendants |
| Each executor | 1 CPU, or 2 with `--executor-cpu 200` | 1 GiB | Supervisor and two single-use Goose slots |
| PostgreSQL | 1 CPU | 512 MiB | Owned postmaster and all backends/background workers |

Swap is disabled for these scopes and each has a 256-task limit. Four ready slots
remain the global running capacity. Per-role ceilings are not CPU reservations:
the roles share four allowed logical CPUs and the host with unrelated work. Their
ceilings are not added into a claimed dedicated allocation. Disk bandwidth, host
page cache and external contention remain uncontrolled. These profiles are not
AWS task sizing recommendations.

The driver verifies actual cgroup membership, `cpu.max`, `memory.max`, swap and
task limits rather than trusting requested values. Each matrix case retains
before/after CPU usage including live descendants, throttling counters, memory
events and charged-memory peaks. An OOM kill fails validation. Memory peaks are
scope-lifetime values; per-phase CPU and throttling are differences. Sampled RSS
is supplementary and may double-count shared PostgreSQL pages.

The scope launcher receives only the user-bus variables needed to create a
transient scope, then removes them before executing the existing fixture command.
It does not add desktop/session access to the tested controller or executor.

## Timing boundaries

`AYATI_TRACE_START=1` and `RUST_LOG=ayati=info` retain the existing sequential
phase timings and add `start_point` events. Only assignment ID, side, a fixed
phase label, version and Unix microseconds are logged. No prompt, source text,
credential or conversation body is added to these events.

Successful pooled starts correlate these points:

1. Controller has a durable offer available for delivery.
2. Executor parses the offer and queues the run task.
3. Executor begins the start HTTP request.
4. Controller enters and returns from the start handler.
5. Executor receives the context, finishes local setup and submits the ACP request.
6. Scripted model server observes HTTP headers, reads the body and decodes input.

The provider timestamps precede/after body parsing separately; first public token
and saved/final text remain distinct measurements. Reports calculate offer
delivery, executor queue, request transport, controller handler, response,
local handoff, ACP-to-provider, body-transfer and decode intervals. An ACP submit
point is a client call boundary, not proof of when Goose writes an upstream
request. The remaining ACP-to-provider interval includes Goose processing and
transport/fixture scheduling.

Unix timestamps are correlated only on this same host. Existing sequential
phase durations use monotonic clocks. Negative interval counts are retained to
expose timestamp/ordering limitations; they are not clamped to zero. Independent
p95 phase values must not be added as if they came from one request. Cross-host
cloud tracing needs a separate clock/correlation design.

## Bounded profile lookup

Controller preparation previously loaded every accessible agent profile and
selected one in Rust. It now fetches at most the requested profile through the
same workspace lock, membership/owner/grant rules and private-agent predicate.
Listing and single lookup share the predicate, with bound ID parameters. Current
authorization remains checked before/after preparation and on start/output/final
operations. No permission cache was introduced.

A PostgreSQL regression compares the selected/listed value and covers grant
addition/revocation, cross-workspace lookup and a member's private agent hidden
from the workspace owner. This bounds profile materialization as the agent list
grows; it is not evidence that the entire startup target has been met.

## Evidence and acceptance

Each runner directory retains `profile.json`, `result.json`, `setup.log`,
`postgres.log`, `run.log`, the copied binary and the stopped cluster. The result
records binary hashes, checkout source hashes, PostgreSQL durability/version,
verified PostgreSQL limits and exit status. Fixture directories printed in the
run log retain detailed `load-report.json`, process logs and fault reports.
Checkout hashes describe the invocation context; the copied binary's hash
identifies the actual executable tested, including when comparing an older build.

The workload still has two repetitions, small/large scoped contexts, concurrency
1/4/8/16 and 24 requests per case: 384 healthy requests per CPU profile. Each
request must retain one attempt/inference, correct scope and final state, and
all correlated points. The final wave at concurrency 16 contains eight requests.
Ready-capacity targets remain p95 dispatch below 100 ms and model request below
250 ms. Overflow queueing is reported separately; target failures remain visible.

Both final matrices passed correctness: 384 requests per profile, 768 total,
with one inference/attempt per healthy request and no conversation/knowledge
leakage. The same instrumented debug executable was used for both profiles (SHA-256
`9888044dbda4d0d3370639e8667a829254ef4549d2a89c0ab6eb3c0c813c7370`).
All recorded same-host interval inversion counts were zero.

| Executor budget | Context | Concurrent | Model-request p95, repeat 1 / 2 | ACP-to-provider p95, repeat 1 / 2 |
| --- | --- | ---: | ---: | ---: |
| 1 CPU / 2 slots | small | 1 | 503 / 563 ms | 432 / 444 ms |
| 1 CPU / 2 slots | small | 4 | 1156 / 1144 ms | 998 / 1032 ms |
| 1 CPU / 2 slots | large | 1 | 545 / 529 ms | 446 / 456 ms |
| 1 CPU / 2 slots | large | 4 | 1000 / 1222 ms | 795 / 1050 ms |
| 2 CPUs / 2 slots | small | 1 | 256 / 251 ms | 179 / 189 ms |
| 2 CPUs / 2 slots | small | 4 | 815 / 539 ms | 611 / 320 ms |
| 2 CPUs / 2 slots | large | 1 | 263 / 287 ms | 181 / 205 ms |
| 2 CPUs / 2 slots | large | 4 | 570 / 727 ms | 347 / 419 ms |

Dispatch p95 across ready-capacity cases was 13–64 ms with one CPU and 12–78 ms
with two, passing 100 ms. **Model-request p95 still misses 250 ms**: 503–563 ms
for one-agent work at one CPU, 251–287 ms at two; four-agent cases were
1,000–1,222 ms and 539–815 ms respectively. Overflow retains separate raw
queueing distributions; it is not included in these ready-capacity ranges.

Summed executor cgroup throttled time across case intervals was 265.03 seconds
at one CPU and 1.15 seconds at two. This is a counter sum across two scopes, not
one request's latency. The allocation comparison supports more CPU for this
specific two-slot workload; shared-host conditions prevent a general cloud
performance guarantee. Extra CPU did not complete the latency gate.

The largest remaining observed interval is after ACP submission and before
model HTTP headers. Controller context p95 also grows under four-agent load
(up to 235 / 296 ms for the one-/two-CPU profiles). Do not assign all elapsed
time to a single function or add independent p95 values. The targeted profile
lookup bounds materialization to one agent, but these runs do not isolate its
individual end-to-end speed contribution.

The exact local Goose v1.54.0 tag (`cd643d2126fffab4c6a6186905211ba868d6a657`)
contains lazy `o200k_base` initialization in `token_counter.rs:205`, reached by
the fallback in `context_mgmt/mod.rs:252`; `agents/agent.rs:2462` checks context
before reply execution. This is a concrete first-use candidate, not a
function-level CPU profile proving its share of the measured interval. The
existing prepared-session path has not demonstrated completion of all that
first-turn work before it advertises a slot. Goose itself was not modified.
Exact source excerpts are retained in the verification directory.

The three bounded fault journeys passed. Controller restart with four running
and four queued requests took 2,413 ms to all final replies; actual executor
loss to replacement draft was 12,008 ms with twelve-second renewable leases.
Cancellation/revocation stopped denied output beside authorized work. These are
local observations, not failover guarantees.

All twenty existing process journeys passed in the bounded rehearsal, including
real browser stream/offline/reload/Stop, private Library sources, permission
revocation, pooled/cold worker recovery, lost acknowledgments and HTTPS startup.
Additional checks passed: 148 default Rust tests, 75 disposable PostgreSQL tests,
four resource/cleanup boundary tests, strict all-target Clippy, debug build,
format/diff/local-link checks and the 499-source-file line check. No new migration,
ordinary-database reconfiguration, AWS resources or paid model calls are included.

Retained run directories:

| Evidence | Runner directory | Detailed matrix fixture |
| --- | --- | --- |
| One CPU | `.tmp/bounded-agent-mlevgzaq` | `.tmp/independent-worker-aQdThl` |
| Two CPUs | `.tmp/bounded-agent-xlzvtbse` | `.tmp/independent-worker-hkwVGM` |
| Faults | `.tmp/bounded-agent-nz74dycu` | Printed per-journey fixtures |
| Twenty regressions | `.tmp/bounded-agent-ssfk5w_q` | Printed per-journey fixtures |

`.tmp/latency-isolation-verification` retains build/test logs, launch pointers,
the immutable binary, source evidence and the final verification manifest. The
first scope-launch smoke failed because the sanitized child environment omitted
the user-bus variables. Its evidence remains at `.tmp/bounded-agent-yyx2fwbq`;
the launcher now supplies those variables only to systemd-run and removes them
before the fixture command. The corrected smoke and final suites passed.

The earlier one-CPU trial at `.tmp/bounded-agent-vys2e4aw` used a different
debug executable after Cargo test/build replaced the target. The final audit
caught the hash mismatch; that run remains retained and is excluded from the
headline comparison. Its launch pointer is `earlier-one-cpu-build-launch.log`.
The one-CPU matrix was repeated with the exact executable used by the two-CPU,
fault and regression suites; only these matching-binary runs are compared above.

Next validate a safe preparation path that completes Goose first-use work
before readiness, without inference during preparation or tenant session reuse.
Identify supported hooks before considering a maintained pinned harness patch;
no such patch is shipped here. Separately evaluate consolidating controller
reads while retaining current authorization at context delivery/publication.
Repeat the same bounded workload after either change. Performance acceptance,
foreground/background reservation, release-image/live-model calibration, cloud
machine identity/autoscaling and hard tenant isolation remain open. Resource
scopes provide budgets; they do not establish tenant OS isolation.

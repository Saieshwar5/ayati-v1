# Repeatable release-image verification

The image verifier builds one local Linux amd64 image, records its immutable ID,
then tests controller/executor roles from those exact bytes. It does not publish
the image, provision AWS, migrate an ordinary database or use a paid model.

## Run

Requires Linux, Docker Engine/Buildx access, Python 3.12+, Node, a working systemd
user manager with cgroup v2, `taskset`, `psql` and local PostgreSQL server tools
as described in the [bounded runner](bounded-agent-load.md). Host Ayati/Goose
binaries and a host frontend build are not required in container mode.

~~~sh
python3 tools/verify-release-image.py
~~~

The default builds with two Cargo jobs per Rust stage, then runs the image,
mixed-workload, recovery and [signed bridge](packaged-worker-bridge.md) journeys,
plus both 384-request ordinary CPU matrices.
`--build-jobs 1..4` adjusts build parallelism, outside request measurements.
`--smoke-only` omits the two ordinary matrices; it still runs the mixed and fault
journeys and signed transport with real prepared Goose. `--image sha256:...` reuses a previously built local image. Mutable
tags are refused. In reuse mode checkout hashes describe the test context, not
proof of how that supplied image was built.

The build context uses the existing explicit `.dockerignore` allowlist. The
image contains the optimized Rust application, built web assets, stock Goose,
the separate pinned preparation variant and its license/build manifest, and the
Python signing bridge with locked dependencies. Runtime
credentials are supplied through fixed secret-file names. The added
`RELAY_SIMULATION_DATABASE_URL_FILE` supports the explicitly selected local
simulation command; it does not enable simulation in ordinary identity mode.

## What the checks exercise

- Restricted service UID/GID 10001, read-only root, dropped capabilities and
  `no-new-privileges`, with writable private state only where needed.
- Actual container cgroup CPU/memory/swap/task limits and OOM status; Docker CLI
  resource use is not mistaken for the daemon-managed container workload.
- Bundled frontend assets, exact Goose versions and prepared-manifest/binary
  hashes. Readiness performs no inference. A missing harness cannot supply ready
  capacity or consume queued attempts; a healthy replacement completes the work.
- Executor and prepared ACP harness environments omit core DB/WorkOS configuration;
  the harness also omits controller bootstrap configuration. An offline negative probe verifies
  the executable itself rejects a core DB credential on an executor.
- Existing mixed chat/background reserve, bounded intake, grant revocation,
  controller replacement, actual executor loss, cancellation and active drain.
- Signed transport through the bundled bridge startup code and real prepared
  Goose, five scoped employee/agent identities, wrong-task/direct-backend denial,
  and the image entrypoint's private-file/startup/shutdown checks. IAM and ECS
  inventory are still synthetic, with only local TLS/socket mappings.
- Ordinary small/large-context load, concurrent requests and overflow. The
  verifier retains latency target misses and does not call them acceptance.

Recovery selectors use durable executor ID/incarnation. Diagnostic process IDs
can repeat in private container PID namespaces and cannot identify ownership.

## Local-only transport and isolated data

Each invocation creates a fresh private PostgreSQL cluster and simulation database.
The host-side admin uses the private peer-authenticated Unix socket. Image roles
use a fresh random loopback TCP port and generated SCRAM credentials: a migration
role receives schema creation permissions; the runtime role receives the existing
restricted grants. Credentials stay in a private 0700 evidence directory and
0600 secret files, outside process arguments and printed reports. Executors receive
no database or WorkOS secret files.

Controller/executor containers use host networking **only for the current explicit
loopback controller transport and local fixture provider**. Each retains its own
PID namespace and root/private scratch filesystem. No project directory, owner
configuration, Docker socket or host PID namespace is mounted into service roles.
This is a trusted local operator rehearsal, not cross-host transport or tenant
OS-isolation acceptance. AWS machine identity/private communication remains a
separate milestone before real cloud assignments/context.

The simulated controller retains its fixture-owned state directory across
replacement, including the synthetic company roster. Executor scratch is tmpfs.
A short-lived, network-disabled setup container changes ownership of only the
fixture-owned secret/state paths to the service UID. It has only the ownership
capabilities needed for that setup; service roles run without those capabilities.
The setup helper does not receive a prompt or execute model-supplied code.

Driver/PostgreSQL use verified user scopes. Containers receive the named profile's
actual CPU/memory/task limits, swap disabled and the same four-CPU affinity.
The signed gateway/bridge fixture runs in an additional restricted image container
at 0.5 CPU, 256 MiB and 128 tasks, with only its synthetic fixture files mounted.
Shared host CPU/disk contention remains; these ceilings are not dedicated
reservations or AWS task sizing recommendations. Builds finish before timings run.

## Evidence and cleanup

The top-level `.tmp/release-image-*` directory retains `verification.json`, build
logs, image ID and pointers to each bounded run. Build-input hashes, base commit,
dirty status and whether the image was built or supplied are recorded. Each
bounded run retains its private cluster, Node logs, source hashes, image metadata,
owned-resource registry and container audit. Fixture directories retain the
existing per-request load/mixed/fault reports and process logs. Environment reports
contain names, not secret values.

Containers are registered before creation and labelled with the invocation's
random session ID. Cleanup accepts only registered names with exact session and
image identity; no global stop/prune is used. Normal stops test graceful drain;
final cleanup force-removes only owned leftovers and then stops the private
postmaster/user scopes. Files/reports and the built image/cache remain for review.

These scripted-provider checks do not establish real-model first-token latency,
AWS scaling, private remote identity, business-effect recovery or a multi-hour soak.
Those remain separately bounded deployment gates.
The [provider calibration runner](provider-calibration.md) now prepares the next
gate with actual browser timing and bounded requests; its scripted rehearsal
does not substitute for the operator-selected live-provider run.

## Verified image and results — 2026-10-24

The full command passed with local image ID
`sha256:28e8fac69dd059e56c4a038ac01261393caae0ed07581686626ddece9a0110f5`.
Top-level evidence: `.tmp/release-image-w9pyi7ut/verification.json`.
The initial full build and subsequent cached verification builds succeeded;
the packaged Rust and frontend are built inside the pinned builder images.

All 13 process journeys pass: eleven image/mixed/recovery checks and two ordinary
load matrices. The mixed workload completes 96 short chats beside three held
background calls and a queued background request. Each ordinary matrix completes
384 healthy requests. Every ready-capacity provisional target passes.

| Profile | Dispatch p95 | One-agent model-request p95 | Four-agent model-request p95 |
| --- | ---: | ---: | ---: |
| 1 CPU per two-slot executor | 6–25 ms | 35–45 ms | 74–101 ms |
| 2 CPUs per two-slot executor | 7–18 ms | 33–50 ms | 83–104 ms |
| Mixed, 2 CPUs, chat reserve 1 | 6–8 ms | 34–38 ms | Not this workload |

At concurrency 16, ordinary overflow model-request p95 remains 1,698–1,956 /
1,434–1,523 ms. All 768 ordinary handoff traces have zero observed inversions.
Summed executor throttling across measured intervals is 42.74 / 0 seconds.
Model-request arrival is not a live-provider first token or a cloud SLA.

Actual prompt sizes are retained: approximately 4.9 kB small / 23.3 kB large
JSON message bytes in this image profile. These differ from earlier repository-
native runs, so timing differences are not a controlled claim about container or
policy speedups. Pinned Goose also supports working-directory/ancestor repository
hints; the service image has no checked-out host repository/owner files mounted.

| Evidence | Runner | Main detailed fixture |
| --- | --- | --- |
| Eleven image/mixed/recovery checks | `.tmp/bounded-agent-c0h508yi` | Per-journey paths in run.log |
| One CPU | `.tmp/bounded-agent-1ue1_xea` | `.tmp/independent-worker-SRrJZF` |
| Two CPUs | `.tmp/bounded-agent-ih4fwtne` | `.tmp/independent-worker-FnLP3S` |
| Packaged artifact/environment hashes | Same image suite | `.tmp/independent-worker-HrI5WP/image-report.json` |

The packaged Ayati ELF SHA-256 is
`a62a227a2a3f9f91c960fdeff975d80389b960e011e0ccc5e3881d349f656a7d`;
prepared Goose is `320d3c2d17e563f71e83d8ab68ea2fca2c3ecb3dd77b9146ec32d9323bae606e`,
built with Rust 1.98.0. All service roles use the same immutable image ID.
Locked sources and pinned base images reduce drift; package repositories and
build metadata can still vary. Verify each newly built artifact rather than
assuming bit-for-bit reproduction or rebuilding a different image for deployment.

Nine Python resource/ownership guards pass normally and under `python -O`.
Three native recovery journeys pass after the identity-aware fixture updates.
The first image run retained one PID-translation test failure; the corrected full
workflow passes without weakening cancellation assertions. Initial/intermediate
logs remain in `.tmp/release-image-verification` and its printed run directories.
The independent final cleanup audit is
`.tmp/release-image-verification/final-audit.json`: all registered test containers,
private postmasters and scopes are stopped; the 16 pre-existing stopped containers
remain. Docker Engine 29.7.2 and Buildx 0.37.0 were used.
This supplements the older [Compose polling-worker journey](hosted-deployment.md);
it does not replace its dispatch mode or imply cross-host pooled authentication.

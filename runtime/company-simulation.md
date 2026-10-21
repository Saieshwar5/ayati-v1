# Persistent local company simulation

This increment provides a local company rehearsal, not a production certification.
The normal `identity` server does not enable simulated login or scripted execution.
The original workspace/database is not modified by the simulation helper.

## Start and inspect

From the repository root:

```sh
cargo build --locked
npm run build --prefix web
./tools/company-simulation.sh setup
./tools/company-simulation.sh serve
```

Open `http://127.0.0.1:8876/index.html?view=simulation`. Choose an employee; Ananya
Rao is the owner and can see Company → Simulation results. Feedback contains
permission-scoped findings. The account menu and simulation banner switch employees.

The helper uses a separate PostgreSQL cluster on a private Unix socket, port 55433,
with no TCP listener. The application runs as `relay_app`, without schema or audit
update/delete authority. It uses the existing local PostgreSQL tool installation;
set `PG_BINDIR`/`PG_SHAREDIR` if it is elsewhere. The simulation database name must
begin `relay_simulation_`; the data directory must be empty or already marked for
that database. It refuses an existing ordinary data directory. Keep it loopback-only:
the explicitly unauthenticated employee chooser is a local testing control.

```sh
./tools/company-simulation.sh run
node tools/company-simulation/browser.mjs
node tools/company-simulation/load.mjs 90
node tools/company-simulation/findings.mjs
```

The runner provisions 20 named employees, five teams, 20 shared agent identities,
private department channels, a public announcement channel, cross-team project and
incident conversations, and ten canonical human DMs. Agent identities are separate
from the four execution slots. No VM, model inference, email or external business
service is used. The browser check uses installed Chromium and fake microphone
input. The synthetic delivery-image fixture uses the existing Python/Pillow tooling
(verified here with Pillow 12.3.0). No image/audio understanding is claimed.

Run data stays under `.ayati-local/company-simulation/`:

- `data/company.json`: stable fictional employee identities.
- `data/workspaces/<workspace>/objects/`: immutable local object bytes.
- `postgres/`: separate company database cluster.
- `runs/<run>/`: manifests, checks, request timing evidence, screenshots and reports.
- `backups/`: database dumps, files, reports and restore verification.

Nothing automatically deletes old runs. New rehearsals create new channels while
retaining earlier history; DMs remain canonical across rehearsals. The report distinguishes
API timing, browser visibility and deterministic business-operation results.

## Product capabilities

New shared-company features require PostgreSQL. SQLite/private-agent behavior stays
available, but it does not implement these new collaboration APIs. Apply the fourth
through sixth explicit forward migrations before using the new binary with an ordinary
PostgreSQL workspace. The helper migrates only its dedicated simulation database;
it does not migrate the user's ordinary workspace.

- Shared agent profiles can be created without provisioning private VMs. Owner
  grants still control which employees can request them. Real Goose execution of
  these agents remains conversation-only; the scripted executor is simulation-only.
- Teams organize people; team membership does not implicitly grant channel access.
- Channel member replacement rechecks current workspace roles. Managers can rename
  channels, switch private/public visibility and restrict posting to managers.
  Making a channel public exposes existing history and published attachments.
- History has a bounded 200-message page and an older-history cursor. Referenced
  thread roots accompany pages. UI merges loaded pages by stable message identity.
- Files up to 10 MiB, at most eight per message, have scoped metadata, checksums,
  safe names and immutable UUID object keys. Uploads remain uploader-only until
  published in that conversation. Cross-channel attachment reuse is rejected.
  Current channel/workspace access is checked again on download. Only recognized
  passive image/audio formats render inline; other formats download.
- The local object store is an integrated filesystem implementation, not S3 or an
  independently deployed object-storage service. It intentionally keeps orphan bytes
  after uncertain metadata writes. Quotas, reconciliation and garbage collection remain.
- Feedback stores expected/actual outcomes, impact, category, evidence and status.
  Its audience follows the linked conversation, including revocation. Exact retries
  are idempotent. Manager status changes use revision checks and audit entries.
  The list is currently limited to the latest 200 accessible reports.
- Internal quotation drafts have real persisted totals, requester/assignee, manager
  approval, revision checks and completion state. Scripted agent operations require
  an explicit per-channel work grant, live requester access and an active job claim.
  The business write and receipt commit together; retries reuse that receipt. This
  is not an invoice, payment, inventory or external connector platform.
- PostgreSQL accepts up to 64 pending shared replies, with four executing locally.
  Excess admission returns HTTP 429. The dispatcher reserves a slot before claiming,
  wakes on accepted work and worker completion (500 ms recovery fallback), and retains the existing five-minute claim
  lease, three-attempt recovery and stale-publication fencing.
- Responses carry request IDs and Server-Timing. New jobs record queue/execution
  timings; old rows deliberately remain unmeasured. Owner metrics expose counts
  and measured queue distributions. These queue aggregates span the workspace lifetime,
  including intentionally paused recovery jobs; do not mistake them for per-run latency.

## Verification and measured findings, 2026-10-21

All values below are local debug-binary observations with scripted execution; they
are not throughput or latency promises for real models or a hosted service.

| Check | Result |
| --- | --- |
| Initial company rehearsal | 11/12 scenarios passed; four-agent pending cap rejected 16/20 burst requests |
| Repeat after queue/path fixes | 12/12 scenarios passed; 20/20 burst requests completed; burst drained in about 871 ms |
| Final event-woken dispatcher rehearsal | 12/12 passed; 20/20 burst drained in ~731 ms; 722 requests, median 23 ms, p95 125 ms |
| Final sustained confirmation | 1,419 accepted messages over ~60 seconds; zero loss/errors; 104/104 replies; send p95 ~36 ms |
| Initial request timing | 794 requests; median 30 ms; p95 234 ms |
| Repeat request timing | 730 requests; median 30 ms; p95 177 ms; differing polling counts mean this is not a controlled microbenchmark |
| Sustained employee workload | 2,128 accepted messages in roughly 90 seconds; zero missing messages or send errors |
| Sustained agent outcomes | All 149 requested replies completed |
| Sustained send timing | Median 17 ms; p95 32 ms; p99 89 ms |
| Observed server resident memory | Approximately 38 MiB at beginning and end; short-duration stability only |
| Two-browser delivery | Approximately 2.4–2.9 seconds initially; repeat approximately 2.5 seconds, dominated by polling |
| Browser interactions | Upload, image decode, recording, actual audio playback, feedback submission and narrow/light/dark checks passed |
| Restart retention | 3,153 messages and 10 objects retained, with selected-run message IDs and attachment checksums matching |
| Queued-work restart | 20 accepted jobs survived a real restart and each produced exactly one completed reply |

Verification: 56 Rust library tests, 18 real PostgreSQL tests and all 13 client test
files passed. Production client build, browser-test type checking, strict Rust clippy,
format/diff checks and the 337-file source-length check passed. Browser journeys
actually executed; type checking is not counted as browser execution.

Tests cover current membership, channel/media isolation, public announcement rules,
feedback audience/revisions, exact duplicate requests, internal approvals, revoked
operation grants, stale claims, receipt idempotency, queue capacity and pagination.
The existing PostgreSQL recovery tests inject expired leases; the retained company
restart exercise covers queued jobs, not a five-minute active-worker crash wait.

The final backup restore comparison matched all 24 public tables and sequences;
all 20 object files also matched their backup checksums. The final retained inventory
contains 5,181 messages across the three rehearsals and load/recovery/browser checks.

## Recovery and backup

```sh
node tools/company-simulation/retention.mjs capture
# Stop the simulation server with Ctrl-C, then start it again.
node tools/company-simulation/retention.mjs verify
node tools/company-simulation/recovery.mjs prepare
# Dispatch is now paused, with 20 persistent requests. Restart the simulation server.
node tools/company-simulation/recovery.mjs verify
```

Stop the simulation server and runner before a coordinated backup:

```sh
./tools/company-simulation.sh backup
python tools/company-simulation/restore-check.py
./tools/company-simulation.sh serve
```

Restore checking creates and retains a separate database, compares every public
row and sequence, and writes `restore-result.json`. It never overwrites the source.
The recovery Git tag `recovery-before-company-simulation-2026-10-21` points to
`55ea5366`; it protects source history, not database/file records. Database rollback
requires a matched backup, not merely checking out that tag.

## What the evidence says to improve next

1. Message visibility is much slower than message persistence. Evaluate incremental,
   permission-aware delivery with revocation/reconnect tests; avoid simply increasing
   full-history polling across every employee.
2. Do not add distributed workers or a message broker based on this workload alone.
   The current PostgreSQL boundary handled it. Measure multi-workspace contention,
   longer runs and realistic execution durations before relaxing locks or scaling out.
3. Add unread/notification behavior, complete older-thread navigation, feedback
   pagination, richer business workflows and more natural scenario variations.
4. Add storage quotas, orphan reconciliation, larger-file/concurrency measurements
   and an object-store adapter only when deployment requirements call for one.
5. Extend internal work operations to real runtime adapters behind the same
   authorization and receipts. A scripted quotation does not prove real-model judgement.
6. Run multi-hour and resource-exhaustion/outage exercises. Current tests do not prove
   absence of memory leaks, production rate-limit safety, real-provider availability,
   OCR/transcription quality, or whether actual employees will find the product useful.

## Reliable messaging follow-up

The earlier polling measurements above are retained baseline evidence. Shared chat now
uses durable live synchronization, a local outbox, delivered/seen receipts and unread
counts. See [the current contract and outage checks](reliable-messaging.md). Additional
`messaging-*` reports and conversations are retained alongside the original rehearsals.

# Local workspace acceptance

Run the current workspace journeys sequentially in disposable local infrastructure:

```sh
cargo build --bin ayati
npm --prefix web run build
python3 tools/run-agent-load.py --suite workspace
```

The existing local dependencies must already be available: PostgreSQL server tools,
pinned normal/prepared Goose and the project's installed Chromium. The runner uses
actual Goose against scripted loopback inference and local MCP fixtures. It does
not activate a provider, send email/WhatsApp, use paid inference or deploy cloud
resources. Do not interpret synthetic-model responses as business-quality evidence.

## Coverage

The suite currently contains 17 connected cases across 14 files:

- Trusted requester/message attribution and isolated older-history retrieval.
- Running/queued work across browser-session expiry and controller restart, followed
  by real grant revocation; login expiry does not silently cancel accepted work.
- Managed tool-free and pooled Goose exclusion of host project hints.
- Browser context preview, approved-note audiences, real editing and forgetting.
- Durable ordinary questions, bounded handoffs and source-aware helper reads.
- Signed connected events and scheduled work.
- Personal reviewed Home drafts/layout and source revocation.
- Saved agent samples, Stop and restart behavior.
- Quiesced workspace restore with a waiting question, uncertain external action,
  reviewed Home snapshot and private chat file; restored work does not replay.

Browser journeys include real backend authorization, stale/denied reads and the
relevant desktop/narrow/dark views. They complement the Rust/PostgreSQL and web
unit suites; they are not a replacement for those tests or every product journey.

## Isolation and deadlines

The runner copies an immutable binary and retains hashes of relevant source,
creates a private PostgreSQL cluster/socket and records owned transient scopes.
Each fixture uses its own schema and data directory. Existing demo databases and
processes are not restarted, migrated or stopped. Retained output lives under
`.tmp/bounded-agent-*` and the printed `.tmp/independent-worker-*` directories.

The workspace driver includes real Chromium and has a 1 GiB ceiling. Other roles
retain the existing controller/PostgreSQL 512 MiB and executor 1 GiB ceilings.
These are local test limits, not cloud sizing or exclusive CPU reservations.

The test command has a default 1,800-second deadline, configurable with
`--timeout-seconds 60..14400`. The context suite derives its default from its
bounded rounds/idle settings. A deadline returns exit code 124 and retains a
failure reason; it does not turn an interrupted case into a pass. Setup commands
are outside that suite deadline. Owned cleanup runs after failure and verifies
that recorded scopes are inactive. Cleanup failures produce a nonzero result and
are retained in `result.json`, including when stopping one scope fails.

No shared services can appear in the cleanup list: the complete list must match
this invocation's UUID prefix before any stop command runs. The seven Python
runner tests cover resource boundaries, foreign-unit denial, inactive/unloaded
units, timeout continuation, still-active denial and invalid deadline arguments.

## Evidence on 2026-10-26

The initial 16-case acceptance run passed in approximately 138 seconds:
`.tmp/bounded-agent-vi2mjlay/`. All 51 recorded scopes were independently verified
inactive afterward. This run predates the added structured cleanup result field;
it is not claimed as evidence of that field.

The expanded current-source 17-case suite passed in approximately 170 seconds
(`.tmp/bounded-agent-p00mg8bz/`), including the viewer-isolation fix and new
session-continuity case. Its structured cleanup record verifies every owned scope
inactive. The first standalone session case passed behavior but failed teardown
when the fixture stopped its restarted controller before its worker drained; the
case now explicitly drains the executor while its controller remains available.

An explicit 60-second interrupted context run returned 124 with all seven owned
scopes verified inactive (`.tmp/bounded-agent-9j4twabd/`). The follow-up deadline run
(`.tmp/bounded-agent-tw9sivbw/`) returned 124 and recorded all seven scopes inactive.
The corrected 153-request context smoke (`.tmp/bounded-agent-8rw_607k/`) passed
including three expired-session denials, nine normal sign-ins and verified cleanup. The long context rehearsal,
startup matrix, failure injection and local resource observations are documented
in [multi-workspace-context.md](multi-workspace-context.md) and
[bounded-agent-load.md](bounded-agent-load.md).

Passing these local cases does not establish live-provider eligibility, real-model
judgment, production recovery, cloud cost/availability, full memory provenance or
complete sensitive-data lifecycle management.

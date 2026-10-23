# Durable live replies from shared executors

M1.5c's local streaming slice forwards visible reply text from real pinned Goose
through the controller to the existing React Chat. It applies to the opt-in
[pooled executor path](executor-pool.md). Cold/direct-PG/polling compatibility
workers retain final-result delivery. This does not deploy AWS or enable tools.

## Startup and migration

Run all sixteen migrations through `m20261024_000016_interactive_reserve` with the
migration owner, then reapply runtime table/sequence grants. Existing databases
are not automatically migrated by startup. See [PostgreSQL setup](postgres-workspaces.md).
The migration preserves existing IDs, messages, assignment ownership, capabilities
and completion fingerprints; historical timing fields remain unmeasured.

Use the existing `AYATI_CHAT_EXECUTION=pooled`, private loopback worker-control
configuration and `ayati executor` setup. Rebuild `web/dist` for the new client.
Library questions now retain the selected dispatch mode and accepted deadline,
so pooled Library work uses the same output/authority boundary.

## Output contract

`POST /v1/assignments/{assignment}/output` requires the exact assignment credential.
The body contains `assignment` (existing scope plus session nonce), `sequence`,
`phase` and `text`. `phase` is `waiting` or `responding`; reasoning/tool events are
not accepted. Scope still binds executor/incarnation/slot/slot generation,
assignment/attempt generation, workspace, agent, conversation, thread and reply.

- Sequences begin at 1. A new batch must immediately follow the accepted sequence.
  Exact replays acknowledge the original sequence without changing the draft or
  message revision. A changed replay or gap conflicts. Replays recheck authority.
- A waiting batch has empty text and may only be sequence 1. Responding batches
  contain nonempty text. Text is bounded at 4 KiB per batch and 64 KiB per attempt,
  with at most 4096 batches. NUL, invalid phases and oversized output are rejected.
- The controller locks current ownership, rechecks requester/conversation/agent/
  Library-source access, and stores the batch receipt and draft in one transaction.
  Acknowledgment follows commit. Output never extends a lease or grants authority.
- Assignment completion remains a separate idempotent operation. The message's
  `content` and terminal `state` are authoritative. A draft never marks it complete.
  Completion, cancellation and terminal failure clear the public draft projection.

The executor forwards only ACP `AgentMessageChunk` text from its active local
reply. It verifies UTF-16 offsets, preserves UTF-8 boundaries when splitting, and
normally flushes at 100 ms or 4 KiB. The local observer has a bounded 256-event
channel. Gaps, observer lag, rejected writes or exhausted retry time cancel the
harness and produce an explicit outcome instead of silently dropping output.
Network retries are bounded at three seconds and obey independent run renewal/
cancellation. No network/SQL transaction spans model execution.

## Chat, replay and interrupted work

Pending messages retain `state=queued` for existing admission/result contracts.
The separate `progress` projection contains public draft text, phase and attempt
generation. Status follows observed events: preparing, waiting for a response,
responding or interrupted. The UI offers Stop reply only when `can_cancel` is
present; the existing cancellation route still enforces requester/owner authority.

Every message update increments a database revision. Chat applies newer revisions,
including when a snapshot and live event arrive in reverse order. It updates the
existing message bubble and preserves composer focus/drafts and reading position.
Pending drafts are excluded from delivered/seen receipts on both client and server.

The existing PostgreSQL message clock, notifications, authenticated SSE and
per-account/workspace cursor provide delivery and reconnect recovery. Live/replay
and history reads recheck pending-draft requester/source authority as well as the
viewer's conversation access. Revoked drafts are redacted. The stream may coalesce
intermediate batches into the latest saved snapshot; it does not append replayed
text blindly in the browser.

Lease expiry marks the draft interrupted even with no connected executor. A new
attempt clears its public draft and receives a new generation. Old attempt text
remains in the controller's private output ledger but is never concatenated with
the new reply. Recovery still means bounded tool-free retry, not continuation of
an arbitrary model/tool checkpoint. Confirmed cancellation is terminal.

Output receipts/text have no automatic purge in this slice, matching existing
assignment history. A later explicit retention policy must cover this private
ledger, message events, backups and diagnostics. Executors still have no core DB
credentials. The loopback bootstrap/process boundary is not cloud machine identity
or a hard tenant OS boundary.

## Timings and capacity rehearsal

The assignment ledger now records controller context readiness, first received
harness-handoff report and first accepted visible output, alongside existing
intake/reservation/final timestamps. These are successful-transaction timestamps,
not exact browser render times. The handoff report is not a production model HTTP
request timestamp; the harness does not expose that boundary here.

`tests/reply_capacity.mjs` records 24 fresh bounded conversations with two local
executors/four ready slots and a scripted loopback provider. Fixture instrumentation
separately observes actual model request/first token. The browser journey records
first visible text. The report includes p50/p95/p99, named prompt sizes, sampled
live-process RSS and supervisor CPU; it excludes CPU of already exited harnesses.
It also rehearses short chat beside a held run, eight accepted requests exceeding
four slots, delayed growth to six active sessions, queued ownership and active drain.

Both local p95 observations are retained:

| Run | p95 dispatch | Context ready | Fixture model request | First saved text |
| --- | --- | --- | --- | --- |
| Initial observation | 19 ms | 190 ms | 412 ms | 430 ms |
| Final isolated repeat | 12 ms | 87 ms | 247 ms | 271 ms |

The proposed targets are 100 ms for dispatch and 250 ms for model request. The
initial run missed the model-request target; the isolated repeat met both. No
specific latency optimization accounts for this variation. Consistent acceptance
needs repeated load evidence followed by a separately bounded live-provider
calibration before cloud sizing. The [controlled load rehearsal](load-acceptance.md)
adds that local matrix, narrower context/harness traces and recovery under load;
its historical model-request target misses remain explicit. The later
[generic-preparation follow-up](bounded-agent-load.md#preparation-follow-up) passes
both targets across two repeated CPU profiles, with zero pre-ready inference.
These are fixture observations, not a production capacity recommendation or
live-provider/first-token SLA. Browser timing, ready reserve and cold expansion
require their own representative evidence.

## Verification

~~~sh
cargo test --locked
cargo test --locked --lib identity::postgres -- --ignored
cargo clippy --locked --all-targets -- -D warnings
npm test --prefix web
npm run build --prefix web
npm run check:e2e --prefix web
./scripts/check-file-lines.sh
cargo build --locked --bin ayati
node --test --test-concurrency=1 tests/reply_stream.mjs tests/reply_capacity.mjs
node --test --test-concurrency=1 tests/executor_pool.mjs tests/worker_control.mjs
~~~

Set a private dedicated local `RELAY_TEST_DATABASE_URL` (`relay_simulation_*`).
Process fixtures use restricted runtime grants, actual Goose and synthetic data;
reports, schemas, screenshots and timing JSON remain in the printed fixture paths.
Use an immutable `AYATI_TEST_BIN` copy while Cargo builds other targets. The tests
make no paid-model or AWS calls. Actual browser execution is separate from E2E
type checking. Existing assignment/polling/HTTPS process journeys remain regression
checks; multi-hour/cloud scaling and foreground/background scheduling are unproven.

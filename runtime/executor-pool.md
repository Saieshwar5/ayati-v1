# Local shared executor pool

Shared-tool extension: [governed MCP operations](shared-agent-control.md) use a fresh
scoped tool harness and count cold startup separately. Prepared chat stays tool-free.
Current application schema is 18; earlier measurements below retain their original
source/image/migration evidence boundaries.


M1.5b adds `ayati executor`: a persistent Rust supervisor with bounded, prepared
Goose sessions. The controller accepts shared-chat work, prepares authorized
context and assigns it to a compatible ready slot. PostgreSQL owns claims,
generations, leases and final results. Executors use the controller API and
receive no core database or WorkOS credentials.

This is an opt-in, same-host, tool-free rehearsal. Pooled slots use the separate
[pinned preparation variant](harness-preparation.md), `1.54.0+ayati.1`;
ordinary/cold/VM paths retain stock 1.54.0.
M1.5c adds [durable live replies](reply-streaming.md). AWS identity, remote transport,
capacity autoscaling and release/live-provider speed calibration remain later work.
The [bounded preparation follow-up](bounded-agent-load.md#preparation-follow-up)
passes the named local ready-capacity targets. The [cold assignment path](worker-control.md) and
[polling workers](independent-chat-worker.md) remain available.

## Setup

Run all seventeen migrations through `m20261024_000017_cloud_executors` with the
migration owner, then reapply runtime grants as in [PostgreSQL setup](postgres-workspaces.md).
Migration 13 preserves existing dispatch modes, IDs, assignment history and v12
completion fingerprints. Startup verifies the schema without migrating ordinary
databases. Drain old binaries and their queued work before switching modes; all
controllers accepting work must use compatible binaries and the same dispatch mode.
Queued jobs retain their accepted mode and are not silently converted.

Create the private operator bootstrap file exactly as in [worker control](worker-control.md#migration-and-local-startup).
Set `AYATI_WORKER_CONTROL_URL` and `AYATI_WORKER_CONTROL_KEY_FILE` on the controller
and executors. The listener accepts only explicit loopback IPs; this does not
authorize an exposed network or work across separate container network namespaces.

Configure the controller with its private database and WorkOS setup:

~~~sh
AYATI_CHAT_EXECUTION=pooled AYATI_WORKER_PROVIDERS=openai \
  cargo run --locked -- identity
~~~

Build `python3 tools/build-prepared-goose.py` first. Migration 15 withdraws old
unprepared ready slots while preserving owned work; replace older executors after
upgrading the controllers. See the preparation guide for rollout restrictions.

Start each executor in a separate private data directory with the prepared Goose
binary, provider/model configuration and provider key set privately:

~~~sh
AYATI_RUNTIME=goose AYATI_GOOSE_PROVIDER=openai AYATI_MODEL=your-supported-model \
  AYATI_DATA_DIR=.ayati-local/executor-one cargo run --locked -- executor
~~~

Use an isolated environment and an empty private `.env` fence: normal dotenv
discovery otherwise loads a parent `.env`. Executors refuse core database URLs and
WorkOS keys, and the Goose command uses an explicit environment allowlist. The
bootstrap/executor/slot credentials stay in the Rust supervisor. Goose receives
the configured model key; per-run model brokerage is still future work.

| Setting | Default and bounds | Owner |
| --- | --- | --- |
| `AYATI_EXECUTOR_SLOTS` | 2; 1–16 | Each executor |
| `AYATI_PREPARED_GOOSE_BIN` | `.cache/goose/v1.54.0-ayati.1/goose` | Executor; preparation variant only |
| `AYATI_GOOSE_PROVIDER`, `AYATI_MODEL` | One immutable provider/model profile per executor | Executor |
| `AYATI_EXECUTOR_CAPACITY` | `AYATI_ASSIGNMENT_CAPACITY` (2); 1–1024 | Controller, global active pool limit |
| `AYATI_EXECUTOR_WORKSPACE_LIMIT` | Pool capacity; 1–1024 | Controller |
| `AYATI_EXECUTOR_MODEL_LIMIT` | Pool capacity; 1–1024 | Controller, provider/model pair |
| `AYATI_EXECUTOR_CHAT_RESERVE` | 0; below global/workspace/model limits | Controller; [background admission](interactive-reserve.md) |
| `AYATI_EXECUTOR_EXPIRY_SECONDS` | 15; 6–120 | Controller, readiness heartbeat expiry |
| `AYATI_ASSIGNMENT_LEASE_SECONDS` | 120; 6–300 | Controller, accepted execution lease |
| `AYATI_ASSIGNMENT_RUN_SECONDS` | 180; 1–86400 | Controller, persisted at intake |

Use matching controller policy across replicas. This local stream hub permits at
most 64 connected executor identities per controller. Bounds are safety limits,
not measured capacity recommendations. Start several local executors to rehearse
fleet behavior. This change does not update the existing Compose worker service.

## Slot and assignment lifecycle

1. Register a new executor/incarnation with `pooled:true`. Prepare each slot's
   private scratch, tool-free Goose process, ACP initialization and fresh session.
   Await the generic token-counter preparation acknowledgment before reporting
   `ready` with `preparation_version:1`. Preparation submits no model prompt; it does not prove
   a paid provider will accept inference or have low first-token latency.
2. The controller selects connected, ready provider/model-compatible capacity.
   Under one PostgreSQL admission lock it applies global/workspace/model limits
   and one active owner per agent. Workspaces with fewer active runs go first;
   ties prefer the least recently served workspace, then queue sequence.
3. Persist an offered assignment bound to executor/incarnation/slot/slot generation
   and attempt generation before sending it. The startup offer has a 15-second
   lease. An SSE event is a delivery hint, not authority or proof of acceptance.
4. The executor acknowledges through the existing assignment `start` route using
   its exact slot credential and one session nonce. That transaction prepares
   scoped context and confirms ownership plus a full lease in one transaction. Duplicate start/offer replay
   cannot create another local inference. Each run renews independently every
   500 ms and obeys its conservative confirmed lease deadline.
5. Stop/reap Goose and clean scratch before terminal publication/release. Every
   used session is destroyed. A fresh process/session, token and slot generation
   replace it; readiness never depends on resetting a previous tenant transcript.

An idle agent owns no slot. A ready shared slot still uses compute. One message
uses an already prepared session; replenishment happens after that session finishes.
When compatible capacity is full, the durable job waits without consuming an
attempt. Missing profiles also wait; operators must provision the matching profile.

Pool routes under `/v1/executors/{executor}/{incarnation}/pool` use that executor's
credential: `GET /events`, `POST /capacity`, `POST /drain` and `POST /stop`.
Capacity includes bounded slot identity, generation, provider/model, state, token
and diagnostic PID plus preparation version. PostgreSQL stores credential hashes. A reported PID does not
authenticate a process or independently prove a malicious executor's readiness.
Executors are trusted operator processes in this local boundary.

## Recovery, draining and limits

- SSE reconnect replays durable unacknowledged offers. An old connection's cleanup
  cannot detach its replacement. Delivery queues and frames are bounded; a
  200 ms reconciliation tick supplements immediate intake/readiness wakeups.
- Separate executor heartbeats withdraw all slots after registry expiry. Run
  leases fence each attempt independently. Short controller restarts reconnect
  without repeating active inference; a longer outage stops the harness at its
  safe deadline. A lost registry identity requires restarting the executor.
- SIGTERM withdraws idle capacity and preserves already offered/running work,
  then closes prepared sessions and exits. SIGKILL kills child Goose processes
  through the Linux parent-death boundary. Another executor recovers expired
  tool-free work with a new attempt, up to the existing three-attempt limit.
  Restarting with the same exclusively locked data directory clears old session
  scratch before preparing new slots; abandoned directories on other hosts require
  the deployment's disk/task lifecycle cleanup.
- A newer slot generation cannot displace a still-leased attempt. Unconfirmed
  release leaves readiness blocked until reconciliation; it does not kill other
  healthy sessions. Completion moves the old slot to cleaning, so a delayed ready
  heartbeat cannot reopen its used generation.
  Advancing the slot generation revokes its old capability, including completion
  replay access; the saved receipt and final reply remain durable. Completion
  retries happen before the supervisor advances the slot.
- Cancellation, requester/source revocation, stale scope and expired capabilities
  still reject output. General business effects/checkpoint continuation remain
  disabled; recovery can repeat tool-free inference and its cost.
- Scratch is private and removed after use, but processes share the executor OS
  account and provider profile. This is not a hard tenant OS/microVM boundary.
  Inherited local command/browser/memory/VM/business tools are disabled.

## Verification and next work

~~~sh
cargo test --locked
cargo test --locked --lib identity::postgres -- --ignored
cargo clippy --locked --all-targets -- -D warnings
./scripts/check-file-lines.sh
cargo build --locked --bin ayati
node --test --test-concurrency=1 tests/executor_pool.mjs
~~~

Set a private `RELAY_TEST_DATABASE_URL` for a dedicated local `relay_simulation_*`
database. For process suites use `AYATI_TEST_BIN` pointing at an immutable copy of
the built binary while other Cargo commands run. Fixtures retain disposable schemas,
reports and logs and use real pinned Goose with a scripted loopback model.

M1.5c's [live output and local capacity rehearsal](reply-streaming.md) now extend
this pool. Streaming/replay and the repeated
[prepared ready-capacity matrix](bounded-agent-load.md#preparation-follow-up) have
local evidence. [Chat reserve and mixed admission](interactive-reserve.md) also
have native release evidence. The named provisional targets pass; release-image/live-model
calibration precedes final M0 sizing and M2/M3 cloud
identity/service acceptance. Prepared PID reuse does not
establish a millisecond response SLA, multi-hour soak or cloud scale-out performance.

# Scheduled intake and idle VM release

The second L3 increment keeps intake in the Linux host daemon, outside the agent
VM. It reuses Tokio and the existing SQLite queue; no new dependency or service
is introduced. The current scope remains one local Goose agent and one active job.
This record describes the original developer API proof. The later
[per-agent schedule interface](agent-schedules.md) exposes creation, pause/resume
and run outcomes in Agents → Details → Manage schedules. Multi-agent routing now
selects private agent stores; the original single-agent scope above is historical.

## Local events

`POST /api/intake/events` accepts this exact shape:

```json
{"source":"fixture.shop","event_id":"order-1","content":"Summarize this synthetic order"}
```

The source is 1–64 ASCII letters/digits/dots/underscores/hyphens; event IDs are
1–128 visible ASCII bytes. Content is nonempty, at most 16000 UTF-8 bytes.
The same source/ID and exact content returns the original task, including after
restart or Stop. Changed content returns 409. Different sources have separate
IDs. A receipt, message, scope and job commit together before acknowledgement.
The 32-pending-job bound returns 429 without partially accepting an event.

Events may be saved while the runtime is unconfigured or dispatch is paused;
they cannot boot it until configuration and dispatch permit work. This endpoint
uses the existing loopback/same-origin boundary. It is for owner/local fixtures,
not a public webhook or an authenticated marketplace connector. Future external
connectors need verified identity, grants, payload translation and trusted receipts.
Do not expose the local daemon to the Internet to receive events.

## Schedules

`POST /api/schedules` accepts:

```json
{"id":"79a4dfc0-eaa1-4cdb-bab4-66efac12c0f3","content":"Make a synthetic stock summary","run_at_ms":1792134000000,"interval_seconds":86400}
```

Choose the desired `run_at_ms`: a positive UTC Unix timestamp in milliseconds.
Omit `interval_seconds` for one-time work; fixed intervals are 1–31536000 seconds.
A reused UUID with the same configuration returns its saved schedule. Changed
configuration conflicts with 409. Configuration is immutable in this proof: pause
the old routine and create a new ID to change it. At most 100 schedules are saved.

`GET /api/schedules` returns configuration, active/paused/completed state, next
due time and `last_task_id`. `POST /api/schedules/{id}/enabled` accepts
`{"enabled":false}` or `{"enabled":true}`. Pausing stops queued occurrences,
leaves an already-running job alone and persists across restart. Use task Stop
to cancel that running job. A completed one-time schedule cannot be restarted;
its `completed` schedule state means its one occurrence was accepted, not that
the task succeeded. Inspect the task's outcome separately.

The host checks due schedules once per second, without calling the model while
waiting. Enabled schedules and admitted events can trigger billed model calls
without an open chat page. An occurrence receipt `(schedule ID, due time)`, queued job and cursor
advance commit in one transaction. Full queues and database failures leave due
work saved for a later tick. There is at most one queued/running job per routine.

After downtime or a long job, missed fixed intervals coalesce into the latest
due occurrence, and the next deadline stays aligned with the original anchor.
There is no burst of one job per missed tick. Clock rollback cannot repeat a
previous occurrence. This is wall-clock scheduling with second-level polling,
not a hard deadline service, calendar/timezone/cron parser or distributed scheduler.
Interrupted/failed/stopped occurrences are never replayed; a still-enabled interval
may create its next distinct occurrence. Live write reconciliation remains unbuilt.

## VM ownership and shutdown

`AYATI_VM_IDLE_SECONDS` defaults to **300**, accepts 0–86400, and uses 0 to disable
automatic idle shutdown. It affects VM execution only. Closing the web page does
not stop work. The host machine/daemon must remain on to receive work.

The single worker serializes job dispatch, owner Start/Stop and idle shutdown
through one lifecycle gate. A scoped local lease spans boot, the whole model/tool
turn and pending owner approval. Owner boot has its own temporary lease. Leases
have no expiry that could make a slow live operation appear idle; they are not
distributed TTL leases or cloud failover. Future browser/process ownership must
join this gate/lease boundary before those capabilities can use idle release.

Only when no lease and no dispatchable pending job remain does the idle grace
allow shutdown. Queued work behind an explicit dispatch pause does not pin a VM.
Ayati closes its ACP transport, ends the owned runtime and requests QMP guest
shutdown. The guest disk/workspace/Goose session persist. This is shutdown and
boot, not a RAM snapshot or continuation of interrupted work. Forced termination
is reported. A later admitted job waits for shutdown to finish, boots the guest,
checks readiness, loads its saved session and submits only that new job. VM
failure never falls back to the host, and a failed boot does not retry its job.

| Owner operation | Behavior |
| --- | --- |
| Task Stop | Stop that job permanently; future routine occurrences remain enabled |
| VM Stop | Cancel current/queued jobs, persist dispatch pause, interrupt boot, shut down |
| Event during dispatch pause | Save it as queued; keep the VM off |
| Due schedule during dispatch pause | Keep it due without creating or advancing an occurrence |
| VM Start | Boot and deliberately resume dispatch, including saved event/due work |
| New accepted chat | Deliberately resume dispatch; a retry of terminal chat cannot resume it |
| Service shutdown/crash | Preserve queued work/schedules and any existing dispatch pause; interrupt active work |

Chat is rejected during explicit VM Stop's transition so a new message cannot
race its pause. Events can still be durably saved. `GET /api/vm/status` adds
`active_leases`, `dispatch_paused`, `idle_timeout_seconds` and `stopping` alongside
existing process, boot and RSS/CPU observations. A null ownership/control field
means unavailable state, not permission to dispatch or sleep.

## Checks and limits

`tests/scheduled_intake.rs` exercises schema-3 migration, concurrent duplicates,
conflicts, queue backpressure, transaction rollback, coalescing, backward clocks,
routine overlap, pause/resume and Stop across restart. `tests/goose_scheduling.mjs`
uses actual Goose against a scripted provider for event and timer delivery.
`tests/vm_idle.mjs` requires an explicit disposable real-VM manifest and exercises
resource release, history/wake, model/approval leases, shutdown-arrival races,
Stop during boot, a persisted off-state pause and configuration failure.

Verified locally on 2026-10-16: 40 Rust tests, 10 web tests, 13 release Goose host
scenarios, all 22 release real-KVM scenarios, 8 prototype browser journeys and
HTTP smoke pass. Formatting, clippy with warnings denied, production web/release
builds, e2e types and the 80-file source-length check pass. The model is scripted;
Goose, MCP, the guest/disk, web-app browser tests and shutdown/wake paths are real.

One laptop observation with the one-second test grace: QEMU RSS before release
515172 KiB (about 503 MiB); host daemon idle RSS 15424 KiB (about 15 MiB); guest
boot 5.60 seconds; event acceptance through completion 6.47 seconds. The brief
1.1-second host-idle CPU sample was 0.9% of one CPU. The QEMU process exited and
the host remained available without extra model requests. These uncontrolled
samples depend on hardware, boot/cache activity and measurement resolution;
they are not production capacity, cost or latency guarantees.

No real seller accounts, live inference, agent browsing, external writes, production
authentication/isolation, cloud billing savings or uptime SLA are established.
Receipts/history have no retention policy or disk quota yet. Cloud readiness,
backup/restore, guest/runtime upgrade rehearsal and live-provider acceptance
remain later gates. Schema **4** rejects newer databases; older binaries reject
this upgraded state. Preserve compatible backups before migration.

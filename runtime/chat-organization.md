# Personal Chat pins and channel archive

Implemented and verified locally as the serial increment following
[reader PR 69](https://github.com/Saieshwar5/ayati-v1/pull/69), commit `02173530`.
The ordinary workspace database was not migrated.

## Personal organization

Pins belong to one user in one workspace. Conversation IDs identify channels and
person/managed-agent direct messages (`{"kind":"chat"}`). Pinning never creates a conversation or starts a
request. A pinned entry appears once, in Pinned, with its unread state. Archived
channels leave active Pinned/Channels and remain reachable in an explicit Archived
section. Their saved personal pin preference can return on restoration.

Ordered pins persist in PostgreSQL with a revision and exact last-request receipt.
Use a small technical bound of 30 pins, reject duplicates and unknown target kinds,
and validate all targets against current conversation access. The server
filters unavailable entries on reads. An Owner role or old preference cannot
make an otherwise private source visible. Role/source changes and workspace/account
switches clear affected client state.

Pin/unpin are explicit actions. Initial order follows pin time and ordinary activity
cannot reshuffle it. Custom reordering remains optional later work. Failed saves
retain the same exact intended change for an explicit retry. Compare revisions
and reuse a request identity only for exactly the same payload. A stale write
cannot overwrite a newer order; no blind retry or inference occurs on reads.

## Shared channel lifecycle

A channel creator or workspace Owner with current source access can archive or
restore a non-direct channel, matching the existing membership-management rule.
The review shows the current visible unfinished-request count and explains that
already accepted work may continue. Channel settings and agent grants remain separate. Archive uses a revision and
an exact request receipt, records actor/time and writes an audit event. Repeated
acknowledgment cannot repeat the lifecycle change; a stale opposing request cannot
reverse a later decision.

Archive retains history, files, visibility, memberships, receipts and existing
work. It is not deletion or agent pause. New ordinary root/thread messages,
interactive/background agent intake and new attachment publication are closed.
Already accepted tasks may finish, resume their original approval/question wait,
or be explicitly stopped under their existing authority. Existing approval and
question controls stay visible in source history; the guarded approval inbox also
remains available. Source revocation still invalidates work normally.

Enforce the lifecycle in the common transactional intake after exact replay
recognition, so a lost acknowledgment for an already accepted message can be
confirmed after archive. Archive/intake must serialize under the existing workspace
writer transaction. A database guard also rejects accidental new user or
queued-agent message inserts into an archived source; authorized existing result
and approval-reference delivery must continue. Validate upload acceptance again
when saving its object record after storage I/O; retain orphan evidence on uncertain
failure. Direct conversations cannot be archived. Current Library question intake
uses private direct conversations and must retain that separate source boundary.

Restore opens future intake only. It never restarts stopped jobs, changes grants,
answers questions, approves operations, retries uncertain effects or creates work.
Access/membership administration remains available while archived, especially
revocation. Current channel/source events notify other sessions; stale clients
remain subject to server checks.

## Interface and validation

The separate OpenPencil [document](../designs/chat-organization/chat-organization.pen)
and [desktop](../designs/chat-organization/concept-desktop.png)/
[phone](../designs/chat-organization/concept-mobile.png) concepts were selected,
saved and visually checked before application edits.

Follow the existing quiet rail and single workspace surface. Keep rows flat,
accessible pin buttons, useful touch targets, bounded local scrolling and existing
search/new-message flows. Preserve composer drafts/attachments on archive/restore;
show an archived banner and disable new sends in root and thread composers.
Reading, responses, questions, approvals and Stop retain their own controls.

The retained checks cover migration from populated schema 23 without data loss;
personal isolation, pin-time ordering across reload/restart, stale/double saves, revoked targets,
private agent scope, read-only pin reads, lifecycle authority, exact replays,
archive/intake races, background/thread/upload denial, accepted work completion,
question/approval continuation, Stop and restore without revival. The real
Goose/scripted-local-model browser journey verifies the connected interaction and
desktop/phone/dark layouts. No cloud or paid inference was used.

Schema **24** is required for current application startup. No external-channel
provider, new scheduling/delegation adapter or universal cloud acceptance is implied.

## API and migration

- `GET/POST /api/identity/workspaces/{workspace}/chat-pins` reads or explicitly
  saves `{request, revision, pins}`. Pins contain `{kind: "chat" | "local_agent", id}`.
  The response is `{available, revision, pins}`. The existing `/chats` list also
  returns these preferences from the same authorized database snapshot, avoiding
  another startup request or mismatched conversation/pin reads. Runtime catalog
  metadata resolves before that transaction.
- `GET/POST .../chats/{chat}/lifecycle` reads the current state or explicitly saves
  `{request, revision, archived}`. Reads return `archived`, `revision`, `manageable`
  and the currently visible `active_requests` count. Conversation listings add
  `archived`, `archive_revision` and `can_archive`.
- Apply `m20261026_000024_chat_organization` with the existing administrator
  migration procedure and reapply runtime data/sequence grants. It adds the
  membership-scoped preference table, channel lifecycle fields and insert guard.
  Startup never applies migrations. Rehearse against a copy and keep a backup;
  downgrading a running binary is not a schema rollback strategy.
- Existing accepted message retries are acknowledged after archive; fresh message
  IDs are rejected. Upload acceptance is rechecked when saving its object record
  after storage I/O. Existing completed/approval delivery uses its original guarded
  path. Future handoff/scheduling adapters must reuse the common closed-intake
  boundary; this does not claim those new adapters exist.

## Local evidence

95 Rust unit, 155 standalone disposable PostgreSQL, 125 web and 52 local Python
checks pass, alongside Clippy with warnings denied, production builds,
browser-test types and source limits. The fixture-specific controller/MCP unit
requires its own harness and is excluded from the standalone database command.

Eight new PostgreSQL cases cover ordered personal isolation, exact retries,
source/role denial, creator authority, closed root/background/raw inserts,
archive/intake races, old acknowledgments, interrupted waits/Stop, question and
separate approval continuation, object-save rechecks and populated schema-23
upgrade. No task is created by reading or saving pins.

The new real-Goose/authenticated-MCP browser journey checks channel/person/agent
pins without duplicates, a second session and another user's isolation, keyboard
pinning, current-source revocation, archive review without inference, preserved
open draft/files, stale root/thread/background/upload denial, an ordinary answer
and independent exact approval in an archived thread, one confirmed external
fixture write, restore without revival and controller/browser restart. Five
retained questions/Agents/readers process journeys also pass. The editable design
and [checked captures](../designs/chat-organization/README.md) use local fixture
records; no customer data or commercial-provider judgment is implied.

Reproduce focused database checks with `cargo test --locked --lib
chat_organization -- --ignored` and `cargo test --locked --lib
channel_archive_recovery_tests -- --ignored`, using a disposable
`RELAY_TEST_DATABASE_URL`. Run `node --test tests/chat_organization.mjs` with the
built web client/binary. This milestone used schema 24; current startup and release
requirements follow [PostgreSQL setup](postgres-workspaces.md). The inactive
release manifest still has zero active controllers/executors and disabled autoscaling; no cloud
activation occurred. Initial pin-time order is supported; custom reorder and
personal Hide for me remain optional later work.

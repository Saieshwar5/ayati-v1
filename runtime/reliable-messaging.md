# Durable shared messaging

Implemented for PostgreSQL workspace conversations. The private Goose transcript and
SQLite-only mode do not implement these shared-message receipts. Capability discovery
keeps a five-second snapshot fallback for existing SQLite shared conversations. Apply migration
`m20261021_000006_reliable_messaging` explicitly before restarting the new server.
The isolated simulation helper applies it only to its dedicated company database.

## Contract

- **Waiting / Sending**: the exact message payload and stable ID are saved in the
  browser's IndexedDB outbox, scoped to account and workspace. The composer clears
  only after local storage commits. Failed storage retains the draft. Up to 500
  unresolved messages are allowed per local scope. Attachments must finish upload
  first; queued messages retain the uploaded object IDs, not an offline file copy.
- **Sent**: PostgreSQL committed the message. An optional subsequent payload read
  failure cannot turn an already committed send into rejection. An acknowledgement-only
  success stays visible as Sent / synchronizing until the canonical message arrives.
- **Delivered to N**: N other authenticated users' clients acknowledged receiving
  the content/attachment metadata. It does not prove downloaded media or reading.
- **Seen by N**: those users' clients reported the particular message sufficiently
  visible in a focused, visible document for 700 ms. Offscreen messages and unopened
  thread replies are not implicitly seen. This is a UI observation, not comprehension.
- **Checking delivery**: a network/timeout/server failure leaves acceptance uncertain.
  Retry the same ID and payload with exponential backoff/jitter and Retry-After.
  A later definite denial after an uncertain attempt pauses with an explicit
  unconfirmed-delivery explanation rather than claiming the message was never sent.
- **Not sent**: a definite first-attempt rejection. Keep its content and a Retry control.
  Session expiry pauses sends. Agent requests older than 15 minutes require explicit
  review/retry. No client state transfers across accounts or workspaces.

Delivery/read counts represent actual acknowledging users, not “all members”. The
sender can inspect up to 200 named receipts after a current conversation access check.
Reading counts on public channels do not imply that everyone in the organization was
an intended recipient. Read receipts are currently enabled for all shared conversations;
per-organization privacy policies and exact send-time audience snapshots are follow-ups.
Agents retain execution status independently from human delivery/read receipts.

## Live updates and recovery

One workspace-owned EventSource carries message and receipt deltas across feature
navigation. HTTP remains the mutation boundary. No per-conversation 2.5-second history
poller or five-second catalog poller remains in PostgreSQL shared chat. Current history
is retained on temporary failures and cleared when a fresh access check denies it.

Database triggers append change records in the same transaction as message writes,
receipt changes and relevant membership/channel changes. A per-workspace clock row
serializes event allocation through commit; another workspace has a separate clock.
Rolled-back messages cannot leave committed journal records. PostgreSQL NOTIFY wakes
all application instances; LISTEN uses an existing driver connection. Notifications
carry only a workspace ID and are not the source of truth. A two-second replay fallback
covers lost notifications/listener reconnection. The existing application-wide workspace
locks remain; this change does not claim to remove write contention.

Each stream revalidates the session and workspace access on every batch, then filters
messages by current conversation access. Membership events carry no private channel
identifier. Replay batches scan at most 100 events. A one-batch output buffer and
five-second send timeout bound slow consumers. Snapshot/reset and live updates merge
by stable message ID. The tab stores its last applied cursor in account/workspace-scoped
session storage; reconnect replays from it, while invalid/future cursors request a fresh
snapshot. Cursor values are opaque client progress, never permission grants.

Admission limits per application process: six streams per user/workspace, 128 per
workspace, 512 total. Slots release when streams close or shutdown begins. These are
process-local safety bounds, not cluster-global billing/rate quotas. Deploy HTTP/2 and
disable intermediary buffering; SSE responses set private/no-store and X-Accel-Buffering.
No database connection is held per browser between replay batches.

Read acknowledgements contain explicit message IDs, never a blanket “mark latest”.
Seen implies delivered. Updates are monotone and exact retries cause no new journal
events. IDs are batched (100 per request); pending acknowledgements are saved in scoped
local storage and retried. If that storage fails they remain in memory and can be lost
on reload, conservatively leaving receipts unconfirmed. Unread badges include unseen
thread replies without marking them read when their parent is viewed.

## Verification

Run after starting the isolated server:

```sh
./tools/company-simulation.sh test
node tools/company-simulation/messaging.mjs
node tools/company-simulation/messaging-recovery.mjs
node tools/company-simulation/messaging-load.mjs
```

The first browser exercise covers live delivery without repeated history reads,
background delivered-versus-seen, foreground visibility, a committed send with a lost
response and reload, offline sends, recipient catch-up, unopened threads, revocation and narrow layout.
The second starts isolated processes on 8877/8878 against the same simulation database,
verifies cross-instance delivery, stops the sender's server, queues a message, restarts
it and verifies exactly one persisted message. Its directories, shared object-store
links, conversations and reports remain available. The main 8876 process is untouched.

Initial live browser sample: 12 sends, 167–284 ms from browser input/send actions to
recipient display, compared with the previous approximately 2.5-second polling result.
A repeat browser run measured 165–305 ms. The 20-subscriber exercise delivered
100 messages to all 20 clients (2,000 observations), zero missing, delivery p95 281 ms
and max 319 ms; all 19 other users acknowledged each checked sender message. That
measurement starts before submission and uses protocol clients, not 20 rendered tabs.
Two actual server instances delivered across PostgreSQL notifications and a real
shutdown/restart recovered one queued send without duplication. These tests are short;
there is no multi-hour or hosted-capacity claim.

This small local debug-build sample is not a hosted latency guarantee. Look at the
retained messaging-* results for subsequent runs.

## Operating limits

The change journal is retained indefinitely in this increment, honoring the requirement
not to delete simulation records. Production needs measured retention/compaction and a
snapshot-expiry contract before large history volumes. Unread aggregation and receipt
fan-out need larger multi-organization load measurements. The live transcript is bounded
to 400 recent updates plus explicitly loaded older history; very old thread navigation
still relies on history loading.

No service worker/offline application shell is installed: an already open application
can queue while disconnected; loading the entire website from scratch while its server
is down is not promised. Browser data clearing can remove unsent local messages. Failed
uploads are not durably resumable offline. Push notifications, typing/presence, browser
multi-tab connection sharing, per-tenant cluster-wide rate quotas, message editing,
unsend and per-organization receipt policy remain separate features. A stream receipt
is not a cryptographic proof of human attention. Authorization is checked against the
server's current state; it cannot retract information already delivered to a device.

Final browser confirmation passed all eight scenarios, with 12 display timings of
133–283 ms. The final frontend suite has 14 passing test files, including persisted
uncertain sends, exact retries, rejection handling and account isolation.

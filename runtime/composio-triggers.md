# Composio event triggers

Subscription discovery/creation uses Composio Platform APIs and a project key.
The Connect consumer MCP catalog does not expose these controls. Its connector
and schedule paths work separately; a `ck_` key does not establish external
event delivery. Ayati shows this limitation in the trigger editor.

Automation → Triggers manages rules for the selected agent. An owner chooses an
active connected account, registers or selects a Composio event subscription,
names the rule and supplies task instructions. Optional matching uses top-level
scalar equality; the UI currently offers one text-equality field. There is no
script/filter evaluator. Remote registration pins the event version and account.

Sources are shared trusted infrastructure. Multiple agents can subscribe to one
source through separate rules. Pausing/removing one rule does not disable that
source or another agent's rule. Rule operations and run history enforce agent
ownership; stale revisions are rejected. Name/instruction edits cancel older
pending/queued occurrences. Change account, event or filters by pausing the old
rule and creating another. Active tasks retain their Stop control.

## External delivery setup

The daemon still binds only to loopback. Configure a Composio V3 webhook
subscription with the required trigger events and a public HTTPS destination
that forwards **only** `/hooks/composio` to this local daemon. Store the subscription
secret in `COMPOSIO_WEBHOOK_SECRET` and restart. No tunnel, remote endpoint or
subscription has been provisioned automatically by this increment.

Never expose owner APIs or the entire local server through a tunnel. Management
routes retain their existing Host/origin guards. The webhook route alone bypasses
those guards and instead requires the signed provider request. No owner session,
agent selector or account ID supplied in query/headers grants event authority.

Ring 0.17.14 verifies HMAC-SHA256 over the exact `id.timestamp.rawBody` bytes with
the literal Composio webhook secret. Base64 0.22.1 decodes V1 signatures. Both
versions were already in Cargo.lock; they are now direct dependencies without
version upgrades. Ring's Apache-2.0 AND ISC licensing is compatible with this
project. Verification rejects missing/bad signatures and timestamps more than
300 seconds in either direction. Body size is bounded to 64,000 bytes.

V3 metadata must match the saved business user, connected account, subscription
ID and event slug. Webhook JSON is data, never owner instructions or permission.
Trigger-created task text cannot become an owner-message memory source.

## Persistence and recovery

The business database commits the event receipt and matched deliveries together
before acknowledging it. Retries with the same ID/content are deduplicated;
conflicting content is rejected. Unmatched events also get receipts and are not
rematched after a rule is created or resumed. Matching work is capped at 1,000
pending deliveries across the local workspace and 100 non-removed rules per agent.

Each delivery has a stable receipt when accepted by its agent's existing queue.
An agent queue being full leaves the delivery pending. A crash between queue
acceptance and the business acknowledgement returns the same saved job on retry.
Rule revisions fence old queued work even if a pause/edit is followed by restart.
Failed, stopped or interrupted tasks are not replayed. Explicit VM Stop still
parks dispatch; it is not undone by incoming events. Last 20 event deliveries and
linked task outcomes are available in the UI. Matching task input is bounded to
16,000 bytes; larger matching payloads fail without being acknowledged.

Account revocation fences waiting trigger jobs. Already-running tasks can finish,
but later connector execution still checks current grants/account state. Local
rule removal retains its historical deliveries and does not delete remote sources.

## Evidence and limits

Rust checks cover independent agents, exact filters, duplicate/conflicting events,
queue pressure, restart, stale revisions, pause/revocation and memory provenance.
Playwright/real-Goose tests exercise source registration, rule create/edit/pause/
remove, signed delivery, invalid signatures, foreign identity and protected owner
routes. Provider responses and event data are synthetic. No live Composio webhook,
Google source polling, token refresh or cloud reliability is established.

References: [signature verification](https://docs.composio.dev/docs/setting-up-triggers/subscribing-to-events),
[event types and instances](https://docs.composio.dev/reference/api-reference/triggers),
[subscription registration](https://docs.composio.dev/reference/api-reference/triggers/postTriggerInstancesBySlugUpsert).

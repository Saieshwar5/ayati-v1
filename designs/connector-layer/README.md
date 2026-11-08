# Connector layer: production plan (proposal)

Rewritten 2026-11-08 after the owner's decisions: Composio is the provider while
building; Cuckoo must be able to add its own connectors and other kinds of
connection later; the [connector screens](../connector-screens/README.md) are
approved. Pipedream credentials were added to the cloud environment; whether it
becomes a provider now is an open question below. Nothing here is built.
Provider facts were read from docs.composio.dev and pipedream.com on 2026-11-08
and are checked live in phase 0.

## Where we are (main, schema 103)

Working, tested against local fakes:

- One `ConnectorProvider` contract and conformance suite in
  `src/identity/connectors/`. Providers: operator MCP servers and Composio
  (REST v3.1, tool versions pinned per account).
- Cuckoo owns the catalog, approved apps, accounts, rules, approvals, receipts
  and Activity. Agents see only `operations` and `request_action`.
- A saved connection stores its provider; dispatch uses it.
- Shared events (schema 28): signed Composio V3 intake, receipts, per-agent
  triggers with exact filters, bounded delivery to background jobs.

Gaps:

1. An unconfirmed write blocks every later write on that connection, and nothing
   clears it (`postgres/control_actions.rs`).
2. Composio names and fields were never checked against a live account.
3. Two providers cannot offer the same app (`connectors::apps`).
4. Connector settings are read once at start, unlike web search and computers,
   which use a providers file read on every use.
5. Reconnecting always resets rules.
6. Plain fields only; no lists or files.
7. Event sources come from an operator file, one URL and secret per source:
   `POST /hooks/composio/workspaces/{w}/{source}`. Composio delivers every event
   of a project to **one** URL with **one** secret, so this cannot scale past
   hand-made sources. Nothing subscribes at the provider; nothing hears
   `composio.connected_account.expired`.
8. The Apps screens need the approved changes.

## The controller's job

The controller (`ayati identity`) is the only process that holds provider keys
and talks to providers. Executors run Goose and never see keys, tokens or
provider names; they reach business tools only through the controller.

| Job | Today | After this plan |
| --- | --- | --- |
| Hold keys | Key files, read per call | Providers file plus the Control secrets store |
| Choose a provider | First provider listing the app | Route per workspace and app |
| Check before a call | Switch, fields, provider prepare, authority | Same, plus circuit breaker and rate limit |
| Record a call | Receipt | Receipt with dispatch key, latency, cost |
| Notice trouble | At the next call | Expiry events, nightly drift and health checks |
| Receive events | One URL per hand-made source | One URL per provider, routed by identity |
| Subscribe to events | Operator does it at the provider | Controller subscribes when a trigger is created |

## Phases

Estimates are engineer-weeks for one person who knows the codebase.

| Phase | What | Weeks |
| --- | --- | --- |
| P0 | Keys, live checks, provider projects | 0.5 |
| P1 | Unconfirmed writes | 1 |
| P2 | Provider-agnostic core | 2.5 |
| P3 | Approved screens | 2.5 (parallel from week 3) |
| P4 | Triggers through providers | 3 |
| P5 | Pipedream provider, if approved | 1.5 |
| P6 | Production readiness | 2 |

About 10–11 weeks for one engineer, 7–8 with the screens in parallel.

### P0 · Keys, live checks, provider projects

- The cloud environment has `COMPOSIO_API`, `PIPEDREAM_CLIENT_ID` and
  `PIPEDREAM_CLIENT_SECRETS`; they load in the next session. For checks they are
  written to 0600 files for `AYATI_COMPOSIO_KEY_FILE`; nothing is logged.
- Composio: confirm a project key (Cuckoo refuses personal `uak_` keys); read
  each catalog tool on v3.1; record versions and fix the catalog.
- Pipedream: exchange the client credentials at `/v1/oauth/token`; confirm the
  project ID and environment; read the Gmail and Slack components.
- One Composio project and one Pipedream environment per deployment
  (development, staging, production), because each project has one webhook URL.

### P1 · Unconfirmed writes

- Dispatch key saved before every write. Settle endpoint
  `POST …/control/actions/{id}/outcome` for the agent's managers and admins.
- Automatic recheck where the provider can find the outcome, using the
  communications outbox lease pattern.
- Screen 4 ("Did this email go out?") and the Activity **Check** entry.

### P2 · Provider-agnostic core

- `AYATI_CONNECTOR_PROVIDERS_FILE`, read on every use like web search and
  computers: providers, key files, routes per app, price per call.
- Routes per workspace (`relay_connector_routes`) over the file's defaults.
- Catalog split: Cuckoo's apps, actions, fields and triggers in one file; each
  provider's mapping in its own file.
- Contract additions: `capabilities`, `identity`, `describe`, and later events.
- Same-account reconnect keeps rules; list fields; per-call cost and latency;
  `ayati admin connectors verify` and the nightly drift report.

### P3 · Approved screens

- Apps panel as the one place to manage a service; drawer removed.
- Add app catalog with server-side search and categories; one-step connect.
- Health tags in agent setup and the Tools tab; fix three stale help lines.
- Server: `/apps` states and account facts, `/apps/catalog`, one connect request.

### P4 · Triggers through providers

See "Triggers" below. Work items:

- Trigger catalog entries under Cuckoo names, mapped per provider.
- `ConnectorProvider` gains `trigger_types`, `subscribe`, `unsubscribe` and
  `parse_delivery`.
- One intake per provider: `POST /hooks/composio` (project secret, routed by
  `metadata.user_id` and `trigger_id`) and `POST /hooks/pipedream/{source}`
  (per-trigger signing key). The current per-source Composio route stays until
  existing operator sources are moved.
- Provider subscriptions owned by the controller, shared by agent triggers with
  the same account, type and settings; removed with the last trigger or on
  disconnect.
- `composio.connected_account.expired` marks the account for reconnecting at once.
- Agent page: create a trigger from a connected app's event.

### P5 · Pipedream provider (if approved)

Connect tokens and links, accounts, running a pinned action component with the
connected account, revoke, triggers; the conformance suite against a fake; one
recorded live run. Used for apps Composio lacks.

### P6 · Production readiness

- Public HTTPS for `/hooks/*` on AWS behind the load balancer and WAF, with
  size limits, a ±300 s signature window and per-provider rate limits.
- Provider keys in the deployment's secret store or Cuckoo Control; rotation
  without restart.
- Circuit breaker per provider, rate limit per workspace and app, alerts.
- Nightly live smoke with Cuckoo-owned test accounts; runbooks per provider.
- Journeys with fakes for connect, call, expiry, trigger delivery, duplicate and
  forged deliveries, two controllers and restart.

## Triggers

Cuckoo already runs agents from two kinds of start: schedules and event
triggers. Provider events become event triggers; nothing else in the agent run
changes.

**Kinds of provider events**

| Provider | Realtime | Polling | Delivery |
| --- | --- | --- | --- |
| Composio | Pushed by the app (Slack, Notion, Outlook) | Composio checks on a schedule; at least every 2 minutes on Composio-managed sign-in (Gmail, Google Calendar) | One project webhook URL; `webhook-id`, `webhook-timestamp`, `webhook-signature` (HMAC-SHA256 over id.timestamp.body) |
| Pipedream | Webhook sources | `timer.intervalSeconds` on the trigger | Per-trigger `webhook_url`; `x-pd-signature: t=…,v1=…` (HMAC-SHA256 over timestamp.body) |

**Setup, from a person's click**

1. A manager picks an agent, a connected account and an event under Cuckoo's
   name ("New email in Gmail"), optional filters and instructions.
2. The controller checks the existing rules: the agent may read that account in
   that destination, and the person may ask the agent.
3. It finds or creates the provider subscription: Composio
   `trigger_instances/{slug}/upsert` with the connected account; Pipedream
   `triggers/deploy` with the external user, `webhook_url` and
   `emit_on_deploy: false`. It saves the subscription and any signing key, sealed.

**Delivery, with no person present**

1. The provider posts the signed event to the controller.
2. The controller verifies the signature and time window on the raw body, maps
   provider identity to workspace, account and subscription, and refuses
   anything that does not match.
3. It saves a receipt keyed by the event ID before answering 200, so a retry
   is a duplicate and a changed retry is refused.
4. It keeps only the reviewed scalar fields (at most eight), matches each agent
   trigger's filters, and queues one delivery per match.
5. The bounded intake starts the agent's background job under the person who
   created the trigger. Every action the agent takes still passes Controls.

**What to expect:** at-least-once delivery, no ordering guarantee, seconds for
realtime events and minutes for polling ones. Pausing a trigger stops
deliveries; deleting the last trigger removes the provider subscription so
polling stops costing money.

## Decisions taken

- Settling unconfirmed writes: the agent's managers and admins.
- Same-account reconnect keeps rules.
- The provider's name is shown to admins only.
- "MCP server" appears in Add app as the next kind.

## Open questions for the owner

1. Pipedream as a second provider now (P5), and its project ID and environment.
2. The production web address for `/hooks/*`.
3. Composio-managed sign-in or your own OAuth apps, per app.
4. The first apps and the first events customers need.
5. How fast polling events must arrive, given its cost.

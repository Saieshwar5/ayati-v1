# Connector layer: build contracts

Shared agreements for building the [production plan](README.md) in parallel.
Each work package owns its files and schema number; anything shared is fixed
here first. Change this file before changing a shared shape.

## Schema numbers

| Schema | Package | Contents |
| --- | --- | --- |
| 107 | Unconfirmed writes | Settling, rechecks, `not_sent` state on control actions |
| 108 | Provider routing | Routes per workspace and app, account identity, connected by and when |
| 109 | Provider events | Provider subscriptions, event sources from subscriptions, signing keys |
| 110 | Provider operations | Call metrics and cost, drift results, circuit breaker state |

Migrations are `m20261108_000107_…` to `…110_…`, registered in
`src/identity/postgres/migration.rs` in that order. When merging, keep all four.

## Shared Rust contract (already on the branch)

`src/identity/connectors/mod.rs`:

- `Account.identity: Option<String>`: the app's stable account identity.
- `Call::run(arguments, key)`: `key` is the control action id, the same on
  every attempt. Providers that support idempotency forward it.
- `Capabilities { sign_in, revoke, list_fields, idempotency_key, find_outcome, triggers }`
  from `ConnectorProvider::capabilities()`.
- `describe`, `find_outcome`, `subscribe`, `unsubscribe`, `parse_delivery`,
  each with a default that refuses or answers `Unknown`.
- `Found`, `Subscription`, `Delivery`, `DeliveryKind`.

`src/identity/connectors/curated.rs`: apps have `category` (one of
`CATEGORIES`) and `description`; apps have optional `triggers`, each mapped per
provider with one to eight scalar fields.

## Who owns what

| Package | Owns |
| --- | --- |
| Unconfirmed writes | `postgres/control_actions.rs` outcome code (new files preferred), settle route, recheck loop, resuming the waiting task; server only |
| Provider routing | `connectors/mod.rs` registry and settings, `connectors/providers_file.rs`, `curated.rs`, `catalog.json` actions, `postgres/workspace_connections.rs`, `/apps` fields except `last_used`, `/apps/catalog`, one-step connect, states on `/control/available` |
| Provider operations | Call metrics, cost and `last_used`, circuit breaker and rate limit in `control::dispatch_with`, `describe` for Composio and MCP, nightly drift, `ayati admin connectors verify`, `tools/connector-provider-check/`, Control's provider health |
| Provider events | `events/`, `postgres/event_*`, `connectors/triggers*.rs`, hook routes, Composio subscribe and delivery parsing, catalog `triggers` entries, trigger creation from an app |
| Pipedream | `connectors/pipedream*.rs`, `fake_pipedream.rs`, its catalog mappings, conformance cases |
| Screens | All workspace-client connector UI: `web/src/react/apps/`, `settings/workspace-connections*`, `connector-safety.tsx`, `agents/create/connectors-step.tsx`, `agents/tools-tab.tsx`, the outcome dialog (`web/src/react/approvals/outcome-dialog.tsx`) and Activity's Check |
| Provider events, client side | The agent trigger form in `web/src/react/agents/automation/`: choosing a connected app's event |

A package that must touch another's file keeps the change minimal and says so
in its commit message.

## HTTP contracts

All under `/api/identity/workspaces/{w}`. Errors are `{error}` with the
existing status codes. Mutations carry `X-Ayati-Request: identity`.

### Settle an unconfirmed write (schema 107)

`POST /control/actions/{id}/outcome` with `{"happened": bool, "note": string}`
(note at most 500 characters). Allowed for the agent's current managers and
workspace admins who can see the request; the requester alone may not when the
action needed someone else. A repeat of the same answer returns the saved
result; the opposite answer is `409`. Response: the action, with
`state` `completed` or `not_sent`, and `settled: {by, by_name, at, happened, note}`.

The action details used by the approval dialog gain
`settle: {can_settle: bool, checks: [{at, result: "unknown"|"happened"|"not_happened"}]}`
for actions in state `uncertain`.

### Apps and catalog (schema 108)

`GET /apps`: each service in `apps[]` gains:

| Field | Meaning |
| --- | --- |
| `state` | `ok`, `reconnect`, `writes_paused` or `paused` (provider switched off) |
| `paused_action` | The unconfirmed action's id when `writes_paused` |
| `last_used` | `{at, agent_name}` or `null` |
| `connected` | `{at, by_name}` or `null` |
| `via` | Provider label for admins only (`"Composio"`), else absent |
| `capabilities` | The provider's `Capabilities` |

`GET /apps/catalog?query=&category=&cursor=` (any member):
`{categories: [{id, name, count}], apps: [{id, name, description, category, kinds: [meaning], actions: n, state: "connected"|"available"|"requested", connection: id|null}], next_cursor: string|null, can_manage}`.
Connected apps sort first, then by name; pages of 30.

`POST /apps/connect` (admins) with `{"app": id, "request": uuid}`: approves the
app, applies safe default rules and starts sign-in. Response
`{redirect_url}` for a sign-in page or `{connection}` when verified at once.
The same `request` returns the same answer.

`GET …/control/available` and `GET …/agent-profiles/{a}/control`: each
connection gains `state` with the values above.

### App events (schema 109)

`GET /apps/{connection}/events` (members who can see the app):
`{events: [{id, title, fields: [string]}]}`, Cuckoo names from the catalog.

`POST /agent-profiles/{a}/event-triggers` also accepts
`{"connection": id, "event": "gmail.new_email", ...}` in place of `source`; the
controller finds or creates the provider subscription and its source, then
creates the trigger with the existing rules.

The trigger list (`GET /agent-profiles/{a}/event-triggers`) gains `apps`:
`[{connection, app, name, account, state: "ready"|"reconnect"|"unavailable"|"no_read"|"needs_allow", detail, read_title, chat, chat_name, visibility, conversations, authority}]`,
the destinations and versions a trigger from that app needs.
`POST /agent-profiles/{a}/event-triggers/{id}/state` also accepts
`"remove": true` (with `"enabled": false`): the trigger is removed for good, and
removing the last trigger of a subscription stops it at the provider.

Composio project webhook (platform admins):
`GET|POST /api/identity/admin/connector-events/composio`. `POST` needs
`AYATI_PUBLIC_ORIGIN` and `AYATI_SECRETS_KEY_FILE` and stores the secret sealed;
`GET` returns `{set_up, setup: {webhook_url, events, changed, changed_by}, webhook_url, public_origin, secrets_key, configured}`.

Provider intake: `POST /hooks/composio` (one per deployment, project secret)
and `POST /hooks/pipedream/{subscription}` (per-subscription signing key). The
existing `POST /hooks/composio/workspaces/{w}/{source}` keeps working for
operator file sources.

## Settings

`AYATI_CONNECTOR_PROVIDERS_FILE`: absolute path to a 0600 JSON file, read on
every use like `AYATI_WEB_SEARCH_PROVIDERS_FILE`:

```json
{
  "providers": [
    {"provider": "composio", "key_file": "/run/secrets/composio",
     "webhook_secret_file": "/run/secrets/composio-webhook",
     "auth_configs": {"gmail": "ac_123"}, "usd_per_1k_calls": 0.5},
    {"provider": "pipedream", "client_id_file": "/run/secrets/pd-id",
     "client_secret_file": "/run/secrets/pd-secret", "project_id": "proj_…",
     "environment": "production", "usd_per_1k_calls": 0}
  ],
  "routes": {"gmail": ["composio", "pipedream"]}
}
```

`routes` lists providers per app, first choice first. Webhook addresses are
built from the existing `AYATI_PUBLIC_ORIGIN`; without it, provider events are
unavailable and the screens say so. The existing
`AYATI_COMPOSIO_KEY_FILE` settings keep working when the file is absent.
Executors refuse the setting.

## Checks every package runs

```sh
cargo fmt --all --check
cargo clippy --workspace --locked --all-targets -- -D warnings
cargo test --workspace --locked
RELAY_TEST_DATABASE_URL='postgresql://root@localhost/relay_workspace_test?host=/home/user/pg-local/sock&port=55432' \
  cargo test --locked --lib identity:: -- --ignored <your tests>
npm test --prefix web   # when web/ changes
npm run build --prefix web
```

Keep every source file at or below 500 lines.

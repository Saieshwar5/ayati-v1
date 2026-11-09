# Connector providers (schema 53, routing schema 108)

Connectors are modular: Cuckoo owns what people see and decide, and a provider
only signs accounts in, keeps their tokens and makes the calls. Composio is the
first provider; operator-configured MCP servers are another. A provider can be
replaced, or an app moved to a connector Cuckoo builds itself, without changing
rules, approvals or receipts. Built 2026-11-01; providers file, routes per
workspace, account identity and list fields added 2026-11-08 (schema 108).

## Who owns what

| Cuckoo owns, and keeps when a provider changes | The provider does, and can be swapped |
| --- | --- |
| The catalog people see and stable action names, such as `gmail.send_email` | Showing the app's sign-in page and storing the token |
| Which apps admins approved | Refreshing expired tokens |
| Connected accounts: who connected them and which agents use them | Making the actual call to the app |
| Rules, approvals, receipts and Activity | Watching the app for events and delivering them signed |
| Which app events start agents, and their fields | |

Agents see only Cuckoo's two business tools, `operations` and `request_action`.
They never see a provider's name, link, tool names or keys.

## The contract

Every provider implements `ConnectorProvider` in `src/identity/connectors/`:

1. **List** the apps it offers a workspace.
2. **Connect** an app: verified at once, or a sign-in page to open.
3. **Finish** a sign-in: check the account and build the reviewed definition.
4. **Disconnect** an account and revoke its access at the provider.
5. **Prepare** an action: check the account still works and the action still
   means what was reviewed, then run it. Nothing is sent before both pass.

A refusal says why: `NeedsReconnect` (expired, revoked or disabled) or
`Changed` (the account or action no longer matches the review). A result the
provider doesn't confirm is `Uncertain` and is never retried; see
[unconfirmed writes](#unconfirmed-writes-schema-107).

The gateway order is unchanged: Cuckoo's checks first (provider switch, exact
fields), then the provider's prepare, then current authority right before
sending, then the call.

Adding a provider is one module implementing the contract, one line in
`connectors::providers()`, an entry in the providers file, and a `providers`
entry per action in the catalog. `capabilities()` says what it supports
(sign-in pages, revoking, list fields, idempotency keys, finding outcomes,
events); screens read these flags, never a provider's name. Composio signs in,
revokes and takes list fields; operator MCP servers do none of these.
`connectors/conformance_tests.rs` is the suite every provider must pass:
listing, connecting, a read, a write, an unconfirmed write, a changed action,
a broken account and revoking. Both current providers pass it against local
fakes.

Events (schema 109): a provider with `capabilities().triggers` also
**subscribes** an account to a catalog event, **unsubscribes** it, and
**parses** a signed delivery into Cuckoo's terms (an app event, an expired
account, or something ignored). Composio implements them with trigger instances
and its one project webhook; the suite in `connectors/triggers_conformance_tests.rs`
covers subscribing, removing, and good, forged, changed, late and repeated
deliveries. See [shared events](shared-agent-events.md#provider-events-schema-109).

## Cuckoo's catalog

`src/identity/connectors/catalog.json` lists the apps and actions Cuckoo
offers through providers, under Cuckoo's own names. Each action maps, per
provider, to the provider's tool and field names, and only those fields are
ever offered to an agent.

| App | Actions | Composio toolkit |
| --- | --- | --- |
| Gmail | Find emails; Draft, Send, Reply in a thread | `gmail` |
| Google Calendar | Find events; Create an event | `googlecalendar` |
| Google Sheets | Find a row; Read spreadsheet details | `googlesheets` |
| Google Drive | Find files | `googledrive` |

When an account connects, Cuckoo reads each provider tool's input schema,
keeps only the reviewed fields, renames them to Cuckoo's names and keeps plain
values (text, numbers and true or false) and lists of plain values, at most 20
(Gmail's cc and bcc, Calendar's attendees). Where a list's items may be text or
an object, only text is offered. Nested objects and lists of objects are left
out. An action whose provider requires a field Cuckoo doesn't offer is left out
too. The tool version is pinned. Before every call the pinned tool is checked
again; any difference refuses the call as `Changed`. Company rules never match
a list: a text or amount condition on a list field simply doesn't apply.

**Account identity.** An app may name, per provider, a read that proves which
account signed in (`identity` in `catalog.json`). After a sign-in Composio runs
it and keeps one field: Gmail `GMAIL_GET_PROFILE` → `emailAddress`; Google
Calendar `GOOGLECALENDAR_GET_CALENDAR` (`calendar_id: primary`) → `id`; Google
Drive `GOOGLEDRIVE_GET_ABOUT` (`fields: user`) → `user.emailAddress`. The value
labels the account ("Connected as orders@…") and its digest is saved. The tool
slugs and Gmail's field are from Composio's toolkit pages; the Calendar and
Drive fields follow Google's APIs and are unconfirmed against a live account.
A read that fails saves the account without an identity.

Tool and field names were checked against Composio's published toolkit pages
on 2026-11-01, not against a live account. A wrong name fails safe: the
action is simply not offered.

## Admin journey

> Since 2026-11-08 admins start from Apps → Add app and connect in one step
> ([Apps page](apps-page.md#what-admins-do)); the sign-in page and its return below
> are unchanged.

Settings, Connections lists catalog apps beside operator MCP servers.

1. **Approve system** makes the app available, as before.
2. **Connect account** returns the provider's sign-in page, and the browser
   goes there. The provider sends the admin back to
   `/api/identity/connectors/return/{id}`. Cuckoo checks the account with the
   provider itself, ignoring the query string, saves it, and returns to
   Settings with "Account connected". A failed or abandoned sign-in saves
   nothing and is removed at the provider. A sign-in is open for 15 minutes,
   only for the admin who started it, and works once.
3. **Safety rules** and agents' Tools switches work exactly as for MCP accounts.
4. **Needs reconnecting.** When the provider reports the account isn't active,
   the action is refused with "The account needs to be reconnected", the agent
   stops seeing its actions, and Settings shows **Reconnect**. The agent's
   current reply is not cancelled. Reconnecting signs in again, replaces the
   account and revokes the old one.
5. **Disconnect** or removing approval stops using the account and asks the
   provider to revoke it. If the provider doesn't confirm, Settings says so.

Reconnecting the same proven account (the same identity digest) whose
reviewed definition and provider are unchanged keeps agents' switches,
workspace rules, agents' own rules and action meanings. Its revision still
moves, so waiting approvals ask again and running work is fenced. Any other
reconnect, including an account without a proven identity, starts over from
the safe defaults.

## Unconfirmed writes (schema 107)

A write the provider didn't confirm, or one still executing after 60 seconds,
is **unconfirmed**: it may or may not have happened. Until it is settled, every
other write on that account is refused, Activity lists it under Needs review,
and a task waiting for it stays waiting. Reconnecting keeps the account's
connection, so it does not clear the hold.

- **A person settles it.** The agent's current managers and workspace admins
  who can see the request answer whether it happened. The person who asked
  can't when the action needed someone else's approval.
  - `POST /api/identity/workspaces/{w}/control/actions/{id}/outcome` with
    `{"happened": bool, "note": string}` (note at most 500 characters).
  - The answer is final: the same answer again returns the saved one, the
    opposite answer is `409`. The response is the action with `state`
    `completed` or `not_sent` and `settled: {by, by_name, at, happened, note}`.
  - `GET …/control/actions/{id}` (the approval dialog's details) adds
    `settle: {can_settle, checks: [{at, result}]}` while it is unconfirmed, and
    `settled` once answered. Each answer is in the Audit log under Approvals.
- **Cuckoo checks the provider.** Each controller asks
  `ConnectorProvider::find_outcome` (key: the action id) about 1, 5 and 30
  minutes after the write became unconfirmed. A check is leased first, so two
  controllers never ask at once, and nothing is sent again. `Happened` settles
  it as completed (`settled_as` `checked`, shown as Cuckoo); `NotHappened` and
  `Unknown` are recorded for the person who answers. Providers switched off are
  skipped until switched on. Providers without the `find_outcome` capability
  are not asked: Composio and operator MCP servers keep the contract's default
  answer, `Unknown`, so their unconfirmed writes wait for a person.
- **Settling** releases the hold, clears Needs review and wakes open tabs. A
  waiting task continues as after a confirmed outcome
  ([durable approval waits](durable-approval-waits.md)). "Not sent" is saved as
  state `not_sent`: the agent is told it did not happen, and to send it again
  only if someone asks (a new request; the same request returns its receipt).

Schema 107 adds the `not_sent` state, `settled_by`, `settled_at`,
`settled_as` and `settled_note` on `relay_control_actions`, and
`relay_control_outcome_checks` (schedule, lease and up to three checks per
action). The screens that answer it come with the connector screens.

## Cuckoo's switch per provider

Platform administrators (`AYATI_PLATFORM_ADMINS`) can switch a provider off
for every workspace at once, for example during a provider's incident:

| Endpoint | Purpose |
| --- | --- |
| `GET /api/identity/admin/connector-providers` | Switch states and configured providers |
| `POST /api/identity/admin/connector-providers` | `{provider, enabled, note}`; switching off needs a note |

While a provider is off, its actions disappear from agents and anything
requested or approved is refused with the note. Nothing reaches the provider.
Running replies continue. Switching it back on needs no other change.

## Providers file and routes (schema 108)

`AYATI_CONNECTOR_PROVIDERS_FILE`: an absolute path to an owned 0600 JSON file,
read on every use like the web search and computer providers files. The
controller checks it, and every key file it names, at start and refuses to
start when anything is wrong, naming every problem. Executors refuse it.

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

- Unknown providers or fields, a provider listed twice, relative paths, a
  price outside 0–1,000 dollars per 1,000 calls, a route naming an app not in
  the catalog or a provider not in `providers` are refused.
- The Pipedream entry builds the Pipedream provider (see Pipedream setup).
- Which provider serves a catalog app for a workspace: the workspace's own
  route, then the file's `routes` (the first configured provider that maps the
  app), then the catalog default (Composio, else the first configured provider
  that maps the app). Saved connections keep the provider they were made with;
  dispatch uses it. One app is offered once, beside operator MCP servers.

Cuckoo's platform administrators set a workspace's route:

| Endpoint | Purpose |
| --- | --- |
| `GET /api/identity/admin/connector-routes` | Workspace routes, each catalog app's providers and default, configured providers with prices |
| `POST /api/identity/admin/connector-routes` | `{workspace, app, provider \| null, note, reason?}`; `null` returns to the defaults. Recorded in the change log |

## Health and limits (schema 110)

Every call the gateway handles is recorded in `relay_connector_calls`: the
workspace, action, agent, connection, app, provider and operation, when it
started, its latency, its outcome (`completed`, `uncertain`, `refused` or
`preflight_failed`), whether the provider failed while Cuckoo checked the
account and action, and an estimated cost. Never arguments, results or tokens.
The price per call comes from the providers file's `usd_per_1k_calls` once
(`connectors::pricing`).

Before anything reaches a provider, after the switch and the field check:

- **Rate limit per workspace and app**: 60 calls a minute (`RATE_PER_MINUTE`),
  counted over the calls table under a lock per workspace and app. Refusals
  don't count. Refused with "This workspace made too many calls to this app in
  the last minute. Nothing was sent; try again in a minute."
- **Circuit breaker per provider**, in `relay_connector_breakers`, so every
  controller agrees. Five provider failures in a row within two minutes
  (`BREAKER_FAILURES`; uncertain outcomes, and errors or timeouts while the
  provider checks the account) open it for three minutes
  (`BREAKER_COOLDOWN_MS`): calls are refused with "Cuckoo paused calls to this
  service for a few minutes after repeated failures. Nothing was sent." Then one
  trial call goes; its success closes the breaker, its failure reopens it. A
  refusal the provider explains (reconnect, changed) is not a failure, and a
  pinned Composio version that is gone is refused as changed. Operator MCP
  servers share one breaker under `mcp`.

The Apps page shows each account's `last_used: {at, agent_name}` (`at` in Unix
milliseconds): the latest call that reached the app.

**Drift check.** `describe` reads, without using the account, what a provider
defines now: for Composio the pinned tool version and the newest one, each
reduced to the reviewed fields with `curated::project`; for operator MCP
servers the `tools/list` schema. Each reviewed action is `ok`, `missing`,
`changed` (with the Cuckoo field names), `new_version` (calls keep the pinned
one) or not checked. A controller runs it nightly after 02:00 UTC under a
lease in `relay_connector_drift_runs`; `ayati admin connectors verify` runs it
on demand, prints a line per account and action and exits non-zero on missing,
changed or unchecked actions. Results are kept 30 days. Agents using an account
whose actions are missing or changed show in Needs fixing as "Gmail changed
since it was reviewed", with Reconnect to review; reconnecting retires the
result. Accounts that need reconnecting or whose provider is off are skipped.

Cuckoo Control's connected-app providers show, per provider, calls, errors,
refusals and estimated cost this month, the breaker's state and the last drift
check's counts. Live checks of the provider accounts themselves are in
[tools/connector-provider-check](../tools/connector-provider-check/README.md).

## Composio setup

- In the providers file: `key_file` (a project API key; personal `uak_` keys
  are refused), optional `auth_configs`, `webhook_secret_file` and price.
- Without the providers file, the older settings keep working:
  `AYATI_COMPOSIO_KEY_FILE` (an owned 0600 key file) and
  `AYATI_COMPOSIO_AUTH_CONFIGS` (`gmail=ac_123,...`). Setting them beside the
  providers file is refused. Key files are read on every request, so keys can
  be rotated without a restart. Executors refuse these settings.
- Without an auth config for a toolkit, Cuckoo uses its single enabled auth
  config, or creates a Composio-managed one.
- `AYATI_COMPOSIO_BASE_URL`: tests only; a loopback fixture.
- Each workspace is one Composio user, `cuckoo-<workspace id>`. An account from
  one workspace is refused in another.
- Account answers from Composio can carry tokens. Cuckoo reads only the id,
  status, toolkit and auth config, and stores only those ids.

## Pipedream setup

Pipedream Connect is a second provider (`pipedream`, built 2026-11-08 against a
local fake; not yet run against a live project). Settings, all controller-only
(executors refuse them):

| Setting | Meaning |
| --- | --- |
| `AYATI_PIPEDREAM_CLIENT_ID_FILE` | Owned 0600 file with the OAuth client ID |
| `AYATI_PIPEDREAM_CLIENT_SECRET_FILE` | Owned 0600 file with its client secret |
| `AYATI_PIPEDREAM_PROJECT_ID` | The Connect project, `proj_…` |
| `AYATI_PIPEDREAM_ENVIRONMENT` | `development` or `production`; one per deployment |
| `AYATI_PIPEDREAM_BASE_URL` | Tests only; a loopback fixture |

The first four go together. The files are read on every request, so the secret
can be rotated without a restart. Cuckoo exchanges them at
`POST /v1/oauth/token` (client credentials, scope `connect:*`) and keeps the
access token in memory until five minutes before it expires; a rotated secret
or a refused token gets a new one. The token is never logged or stored. The
`pipedream` entry of the providers file (designs/connector-layer/contracts.md)
builds the same settings with `pipedream::Settings::from_entry`.

- **External users.** Each workspace is Pipedream external user
  `cuckoo-<workspace id>`. Cuckoo lists accounts only for that user and drops
  any answer naming another user or app, so an account of one workspace is
  refused in another.
- **Connecting.** A Connect token for the user and app (15 minutes,
  `success_redirect_uri` and `error_redirect_uri` set to Cuckoo's return page)
  gives a Connect Link with `app=<slug>`. Before it, Cuckoo notes the user's
  accounts and their update times; finishing takes the one healthy account
  that is new, or else the one Pipedream updated in place. When Pipedream names
  the account by its email address, that is its identity.
- **Calls.** Cuckoo reads each action component and keeps only reviewed plain
  props: text, numbers and true or false. A `string[]` prop, such as Gmail's
  `to`, is offered as one value and sent as a one-item list. Secret, labelled,
  dynamic (`reloadProps`), file and other props are left out, and an action
  whose required prop isn't offered is not offered. The component version is
  pinned; each call checks the account is healthy and the pinned version still
  takes the same props, then runs `actions/run` with the account as the
  component's app prop. Only a reply with no error is a result; a timeout, an
  error, `202` or anything unclear is unconfirmed. Calls take no idempotency key.
- **Disconnecting** deletes the account at Pipedream.
- **Events.** Subscribing deploys the trigger component at its current version
  with `webhook_url` and `emit_on_deploy: false`, and keeps Pipedream's
  per-trigger signing key; without one the trigger is removed again.
  Deliveries need `x-pd-signature` (HMAC-SHA256 of `{t}.{raw body}`) from that
  key within ±300 seconds. Unsubscribing deletes the deployed trigger.

Mapped for Pipedream: Gmail (find, draft, send), Google Calendar (find, create),
Google Sheets (spreadsheet details) and Google Drive (find files), with
component keys and prop names from Pipedream's component sources on GitHub.
Gmail's reply needs a message id, not a thread, and Sheets' find row needs a
worksheet and column, so those two are not mapped.

Unverified until a live run:

- Component keys, versions and prop types as the Connect API returns them.
- That Connect Link honours `app_id` and `app`, and returns through the
  redirect URIs; that account answers carry `external_id`, `updated_at`, and
  the mailbox as `name`.
- The run reply on failure (`error`, `os[].err`) and the status codes used.
- The delivery body. Pipedream documents the signature only. Cuckoo reads
  `{"event", "context"}` and takes `context.id` as the event id (else a
  SHA-256 of the body) and `context.emitter_id` as the subscription (else
  empty: the per-subscription key already binds the delivery). The intake must
  refuse a delivery whose non-empty subscription differs from its URL's.
- Whether deleting an account also stops triggers deployed with it.

## Schema 53

- `relay_control_connections.provider` (`mcp` by default) and `binding`, the
  provider's identity for the account, never a token.
- `relay_connector_reconnects`: accounts that need signing in again. It is a
  separate table because changing a connection row fences running work.
- `relay_connector_sign_ins`: sign-ins in progress.
- `relay_connector_providers`: Cuckoo's switch per provider.

## Schema 108

- `relay_connector_routes`: a workspace's provider per catalog app.
- `relay_control_connections.identity` (a digest of the proven account
  identity), `connected_by` and `connected_at`, backfilled from the audit log
  for existing connections without fencing work.
- `relay_connector_connects`: one-step connect requests and their answers.
- `relay_app_requests.connector`: a member's request for a catalog app.
- `relay_connection_state()` and `relay_paused_write()`: a connection's
  `ok`, `reconnect`, `writes_paused` or `paused` state for screens.

## Checks

- Unit: catalog validity, field reduction, list fields and identity reads
  (`connectors::curated`); the providers file (`connectors::providers_file`);
  route resolution (`connectors::routing`); list arguments
  (`control::model`); the provider suite for MCP, Composio and Pipedream, plus
  Composio-specific cases (`connectors::conformance_tests`); Pipedream props,
  sign-in, ownership, call outcomes, the token cache, events and signatures
  (`connectors::pipedream_*`).
- PostgreSQL (schema 107): `control_actions::outcomes` (settling, who may
  answer, the hold across reconnects, the waiting task and the checks loop
  with a scripted provider).
- PostgreSQL (schema 108): `connector_routes::tests` (routes, saved
  connections keeping their provider, same-account reconnect keeping rules,
  other accounts starting over, the upgrade) and `connector_catalog::tests`
  (Apps states and `via`, the catalog, one-step connect, states on Tools,
  company rules and lists).
- Provider operations: `connectors::drift::tests` (describe and drift
  findings, verify output); PostgreSQL `connector_calls::tests` and
  `connector_calls::drift_tests` (a metric per outcome, the breaker shared by two
  stores, the rate limit, drift results and their Needs fixing cause,
  `last_used`).
- PostgreSQL: `connector_providers::tests` and
  `connectors::pipedream_sign_in_tests`. They cover a full sign-in, a
  gateway call, the switch, an expired account and reconnecting, and failed,
  expired and once-only sign-ins.
- Web: `tests/workspace-connections.test.mjs` covers opening the sign-in
  page, Reconnect, the return messages and an unconfirmed revoke.
- Journey: `tests/connector_providers.mjs`, with real Goose and a local
  Composio stand-in. An admin connects Gmail from Settings in a browser, and
  the agent sends an email under Cuckoo's action name. The switch stops the
  next send. An expired account shows Reconnect and reconnects. No token or
  key reaches the model, logs or database.

## Not built yet

- One account per app per workspace.
- Composio app events are checked against fakes only; the operator's
  event-source file still works for sources made by hand.
- Nested fields and lists of objects are not offered.
- Operator MCP servers appear in the Add app catalog under "Your company's
  systems" and connect in one step without a sign-in page.
- An abandoned sign-in that never returns stays at Composio until removed.
- A connector Cuckoo builds itself: the contract and suite are ready for it.
- Composio and Pipedream map the same apps, so both can't be configured on one
  controller until routes per app choose between them.
- The simpler admin model and the agent Tools step with search need approved
  designs first. In the simpler model, admins make one choice per connector.

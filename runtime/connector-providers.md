# Connector providers (schema 53)

Connectors are modular: Cuckoo owns what people see and decide, and a provider
only signs accounts in, keeps their tokens and makes the calls. Composio is the
first provider; operator-configured MCP servers are another. A provider can be
replaced, or an app moved to a connector Cuckoo builds itself, without changing
rules, approvals or receipts. Built 2026-11-01.

## Who owns what

| Cuckoo owns, and keeps when a provider changes | The provider does, and can be swapped |
| --- | --- |
| The catalog people see and stable action names, such as `gmail.send_email` | Showing the app's sign-in page and storing the token |
| Which apps admins approved | Refreshing expired tokens |
| Connected accounts: who connected them and which agents use them | Making the actual call to the app |
| Rules, approvals, receipts and Activity | Receiving the app's raw events (not moved yet) |

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
provider doesn't confirm is `Uncertain` and is never retried.

The gateway order is unchanged: Cuckoo's checks first (provider switch, exact
fields), then the provider's prepare, then current authority right before
sending, then the call.

Adding a provider is one module implementing the contract, one line in
`connectors::configure`, and a `providers` entry per action in the catalog.
`connectors/conformance_tests.rs` is the suite every provider must pass:
listing, connecting, a read, a write, an unconfirmed write, a changed action,
a broken account and revoking. Both current providers pass it against local
fakes.

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
values only: text, numbers and true or false. Lists and nested objects are
left out. An action whose provider requires a field Cuckoo doesn't offer is
left out too. The tool version is pinned. Before every call the pinned tool is
checked again; any difference refuses the call as `Changed`.

Tool and field names were checked against Composio's published toolkit pages
on 2026-11-01, not against a live account. A wrong name fails safe: the
action is simply not offered.

## Admin journey

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

As with any reconnect today, a reconnected account's agent switches and rules
start over from the safe defaults, because Cuckoo cannot prove it is the same
upstream account.

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

## Composio setup

- `AYATI_COMPOSIO_KEY_FILE`: an owned 0600 file holding a Composio project API
  key. It is read on every request, so it can be rotated without a restart.
  Personal user keys (`uak_`) are refused. Executors refuse this setting.
- `AYATI_COMPOSIO_AUTH_CONFIGS`: optional `gmail=ac_123,...`, for apps that
  sign in with your own OAuth app at Composio. Otherwise Cuckoo uses the
  toolkit's single enabled auth config, or creates a Composio-managed one.
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

## Checks

- Unit: catalog validity and field reduction (`connectors::curated`); the
  provider suite for MCP, Composio and Pipedream, plus Composio-specific cases
  (`connectors::conformance_tests`); Pipedream props, sign-in, ownership, call
  outcomes, the token cache, events and signatures (`connectors::pipedream_*`).
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
- Composio event triggers still come from the operator's event-source file.
- List and nested fields, such as cc or attendees, are not offered.
- An abandoned sign-in that never returns stays at Composio until removed.
- A connector Cuckoo builds itself: the contract and suite are ready for it.
- Composio and Pipedream map the same apps, so both can't be configured on one
  controller until routes per app choose between them.
- The simpler admin model and the agent Tools step with search need approved
  designs first. In the simpler model, admins make one choice per connector.

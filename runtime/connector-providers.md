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

## Schema 53

- `relay_control_connections.provider` (`mcp` by default) and `binding`, the
  provider's identity for the account, never a token.
- `relay_connector_reconnects`: accounts that need signing in again. It is a
  separate table because changing a connection row fences running work.
- `relay_connector_sign_ins`: sign-ins in progress.
- `relay_connector_providers`: Cuckoo's switch per provider.

## Checks

- Unit: catalog validity and field reduction (`connectors::curated`); the
  provider suite for MCP and Composio, plus Composio-specific cases
  (`connectors::conformance_tests`).
- PostgreSQL: `connector_providers::tests`. They cover a full sign-in, a
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
- A second provider and a connector Cuckoo builds itself: the contract and
  suite are ready for them.
- The simpler admin model and the agent Tools step with search need approved
  designs first. In the simpler model, admins make one choice per connector.

# An agent's own connections (schemas 132 and 133)

Built 2026-11-18. First part of moving everything an agent can reach onto the
agent's own page, so the Apps page can go. Approved design: "Worker Page
Redesign" (shown 2026-11-18).

## What changed

- **An app connection can belong to one agent.** `relay_control_connections.agent`
  names it. The agent's owners (or an admin) connect the app from the agent's
  **Connections** tab: Add app, pick it, sign in on the app's own page. The
  connection is on for that agent and no other. Nobody, not even an admin, can
  switch one agent's connection on for another agent.
- **Several agents can each hold their own account of the same app.** The rule
  "one connection per app and workspace" is now one per app and owner
  (`relay_control_connections_owner`). The provider user stays one per
  workspace; each connection keeps its own provider account.
- **One rule set.** An own connection's rules are the connection's rules, and
  the agent's owners change them (`POST …/control/connector-rules`). They start
  as for any new app: reading allowed, deleting blocked, the rest asks first.
  There is no admin limit above them and no second, per-agent rule.
- **The admin's "no" stands.** An app an admin switched off for the workspace
  (`relay_control_systems.approved = false`) can't be connected by an owner. An
  app nobody decided about is allowed the first time an owner connects it.
- **Signing in again and disconnecting** happen on the same tab, by the owners.
- **Every connection belongs to an agent (schema 133).** The workspace's shared
  connections, connected by an admin on the old Apps page, were switched off by
  schema 133 as a new revision, with every agent's switch and rule for them
  removed, so waiting work on them is cancelled. Owners connect the app again on
  their agent's page. New connections without an agent are refused by the API
  (`POST …/apps/connect` and `POST …/control/connections`). A connected system
  (an MCP server added on one agent's page) was already tied to its agent and
  is untouched. The accounts stay at their provider (Composio, Pipedream) until
  someone removes them there.

## The agent page

Five tabs: **Overview** (with the agent's activity, read when asked for),
**Instructions**, **Connections**, **Automations**, **Settings** (model, usage
and budgets). Links to the old Tools, Environment and Activity tabs open their
new place.

Connections has four groups: **Apps** (with Add app), **MCP servers** (the
connected systems), **Computers** (the agent's computer and what it may do),
**Built in** (web search).

## Admins: every connection in one list

**There is no Apps page any more.** Settings › Connections (admins) lists every
app and MCP server connected in the workspace: the account, the agent it belongs
to (linked to its Connections tab), who connected it and when, when it was last
used and whether it needs a new sign-in. **Remove** disconnects an app; an MCP
server is removed on its agent's page. `GET …/apps` gives each one's `owner`
and `revision` for this. Old links to `?view=apps` open Settings.

## Creating an agent

Three steps: **Name and picture**, **What it does** (ground rules and skills,
with the setup chat) and **People**. The Tools and Controls steps are gone:
a new agent starts with web search and its computer and browser on where this
Cuckoo offers them (with the internet, under the workspace's limits) and the
workspace's default model, and its apps and MCP servers are added on its
Connections tab once it exists. A member's assistant starts the same way.

## API

- `GET …/apps/catalog?agent={id}`: the catalog for one agent; `state` is
  `connected` when that agent has its own connection, and `can_manage` is true
  for its owners and admins.
- `POST …/apps/connect` with `{app, request, agent}`: the agent's owner or an
  admin; `agent` is required. The
  sign-in returns to `/?view=agents&selected={agent}&section=connections`.
- `POST …/control/connections` with `{id, server, enabled: false, revision,
  agent}` disconnects an own connection.
- `GET …/agent-profiles/{a}/control`: each connection has `own`, and each
  operation `rule_revision` (the connection rule's revision).

## Checks

- PostgreSQL `connection_owners::tests`: a member connects Gmail for their own
  agent; a second agent gets its own account of the same app; neither sees the
  other's; it is not offered to new agents; nobody connects for an agent they
  don't own or gives a connection to another agent; the owner changes its rule
  and disconnects it and another member can't; a request can't claim it as the
  workspace's; an admin's switched-off app is refused.
- PostgreSQL `connector_routing::tests::the_upgrade_records_who_connected_an_account_and_later_ends_the_shared_connection`:
  schema 133 switches a shared connection off as a new revision and removes its switch.
- Web `tests/all-connections.test.mjs`: the list, whose each is, one Remove
  request, none for people who can't manage.
- Web `tests/agent-connections.test.mjs`: the agent's catalog, one connect
  request and the sign-in page; the own app's rules without an admin limit or a
  switch; disconnect; sign in again; no controls for people who can't change
  the agent.

## Not built yet, and not verified

- Not verified against the real Composio or Pipedream service: that one
  workspace user can hold several accounts of the same app. The fake provider
  used in tests allows it.
- Operators' own MCP servers (the operator file) and company systems (links to
  the company's own web apps) have no screen now that the Apps page is gone.
  An agent's owner adds an MCP server on the agent's page instead.
- An allowed-apps list for admins. Today an admin's earlier "no" to an app
  still blocks it, but there is no screen to set one.
- The store still accepts a connection without an agent (many tests use it);
  only the API refuses it.
- The company's own machines are still added by an admin in Settings ›
  Computers. A simple webhook per agent and one-step MCP setup are not built.

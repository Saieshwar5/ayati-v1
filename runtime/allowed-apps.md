# Allowed apps (schema 134)

Built 2026-11-18. A workspace's admins decide which apps its people may connect
to their agents. Members add the allowed apps, ask for the others, and see the
admin's answer. The decision lives in Cuckoo, not at a provider (Composio,
Pipedream): a provider knows one Cuckoo project, not each workspace's admins.
Builds on [an agent's own connections](agent-connections.md).
Admins can also [add apps from the provider's library](added-apps.md) (schema 135).

## The three states of an app

`relay_control_systems` holds the admins' decision per workspace and app:

| State | Row | A member | An admin |
| --- | --- | --- | --- |
| **Allowed** | `approved = true` | adds it to their agent | adds it |
| **Not decided** | no row | asks an admin | adds it, which allows it for everyone |
| **Switched off** | `approved = false` | sees "Not available here" with the admin's note | allows it again, or adds it |

- **Asking first is the rule.** Until schema 134 a member's first connection
  allowed an undecided app for the whole workspace. Now `connector_connect_begin`
  refuses it: "Gmail isn't allowed in this workspace yet. Ask an admin to allow it".
  Apps connected before schema 134 stay allowed.
- **Switching off ends every connection of the app.** Each agent's connection is
  disconnected, its rules and switches removed and its account handed back to the
  provider to revoke. Allowing the app again does not bring them back: owners
  sign in again. (Before schema 134 only the first connection was ended.)
- **A decision records who made it** (`decided_by`, `decided_at`) and a note of
  up to 300 characters (`note`), shown to members beside a switched-off app.

## Requests

A member asks for an undecided app from **Add app** on their agent's
Connections tab: **Ask admin**, with an optional reason. The request
(`relay_app_requests`, kind `suggestion`, `connector` the app) names the agent it
is for (`agent`).

- One waiting request per member and app. Asking for an allowed or switched-off
  app, or for an agent the member can't manage, is refused.
- **Allow** allows the app for the workspace and answers *every* waiting request
  for it. **Decline** answers that one request with a note; the app stays
  undecided and can be asked for again.
- Allowing or switching off an app in Settings, or an admin adding it to an
  agent, also answers every waiting request for it.
- The admin's note is kept on the request (`note`). Decisions are in the audit log
  (`app_requested`, `app_request_decided`, `control_system_approval_saved`).

## MCP servers

An agent's owner may still connect an MCP server by its address
([connected systems](connected-systems.md)). When that owner is a member, the
server is saved and reviewed but **waits** (`relay_connected_systems.waiting`,
its `relay_control_systems` row not approved): the agent can't use it and nothing
is sent to it. An admin answers it in Settings › Apps: **Allow**, or
**Decline**, which removes it. An admin's own MCP server works at once.

## Screens

- **Add app** (an agent's Connections tab): apps that can be added come first.
  Each tile shows Connect, Ask admin, "Asked · waiting for an admin", or "Not
  available here" with the note. An admin adding an undecided app is told it
  becomes allowed for the workspace.
- **Settings › Apps** (admins; the page was Connections until 2026-11-21): **Allowed apps** above the list of
  connections. "Waiting for your answer" lists app requests and waiting MCP
  servers with Allow and Decline; then every app Cuckoo offers with its state,
  who decided and how many connections it has, with Allow and Switch off.
  Switch off and Decline take a note.
- **Home**: admins see how many requests wait, with a link to Settings. A person
  who asked sees the answer for seven days, with a link to add the app to the
  agent they asked for.
- **An agent's MCP servers**: a waiting server is tagged "Waiting for an admin".

## API

| Endpoint | Purpose |
| --- | --- |
| `GET …/apps/catalog?agent=` | Each app's `state`: `connected`, `available`, `ask`, `requested` or `blocked`; `allowance` (`allowed`, `undecided`, `blocked`) and, when blocked, `note`. Admins see `available` |
| `GET …/apps/allowed` (admins) | `{apps: [{id, name, allowance, note, revision, decided, connections, waiting}], systems: [waiting MCP servers]}` |
| `POST …/apps/allowed` (admins) | `{app, allowed, note, revision}`; revision 0 for an undecided app |
| `POST …/apps/requests` | `{id, connector, agent, reason}` |
| `GET …/apps/requests` | `{requests, mine, can_decide, systems_waiting}`; `mine` is the viewer's waiting requests and the answers of the last seven days |
| `POST …/apps/requests/{id}` (admins) | `{action: approve \| decline, note}` |
| `POST …/agent-profiles/{a}/systems/{id}/state` | `change` also takes `allow` and `decline` (admins, for a waiting MCP server) |

## Checks

- PostgreSQL `allowed_apps::tests`: a member is refused, asks once, and connects
  after an admin's yes, which answers a second member too; a declined request
  keeps the note and the app stays undecided; an admin adding an app allows it;
  switching off ends both agents' connections, shows members the note, refuses
  members and old revisions, and answers waiting requests; the admins' list.
- PostgreSQL `systems::tests::a_members_system_waits_for_an_admin_before_the_agent_can_use_it`.
- Web `tests/allowed-apps.test.mjs`: the catalog states and one request despite
  a double click; a failed request keeps the reason; Allow and Decline; allowing
  and switching off with a kept note after a failed save; Home's lines; waiting
  MCP servers.

## Not built, and not verified

- Not run in a browser or in the cloud yet. The screens were built from the
  existing design system without a separate approved design.
- A member is not told on Home when their waiting MCP server is answered; its
  tile on the agent's page changes.
- No setting to let members add any app without asking, and no limit on which
  account (for example only company email addresses) may sign in.
- Requests for an app outside Cuckoo's catalog have no screen; an admin
  [adds the app](added-apps.md) instead.

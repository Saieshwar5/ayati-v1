# Apps page (first part)

**Apps** is in the rail between Library and Settings. It shows every service the company
connected, what agents may do with each action and which agents use it. Connections moved
here from Settings. From the Apps plan (`proj-docs/apps/`, AP1–AP2, draft decision D243).
Built 2026-11-05. Company systems and built apps come in later parts.

## What everyone sees

- **Cards** for each approved, connected service: a colour per app (stable by server id),
  its name, the account (admins only), the faces of the agents that use it, and
  **Reconnect** when its sign-in expired.
- **Search** by name, and a **"N need you"** pill that shows only apps with a problem.
- **The side panel** for the selected app: what agents can do (each action with its kind
  — reads, changes, money, sends outside, deletes — and its Allow / Ask first / Block
  rule), which agents use it, and how many company rules cover it with a link to them.
- Members read all of this and can't change it. The server applies the same boundary.

`GET /api/identity/workspaces/{w}/apps` returns `{apps, can_manage}`; each app has
`id, server, name, kind ("service"), provider, account (admins only), status
("connected" | "reconnect"), actions [{id, title, meaning, mode}], agents [{id, name,
avatar}], rules`.

## What admins do

- **Add app** opens a drawer with the existing connection controls: approve a system,
  connect an account (including a provider's sign-in page), set each action's safety rule
  and kind, reconnect and disconnect. Their contracts are unchanged
  ([workspace connections](workspace-connections.md), [connector providers](connector-providers.md)).
- A provider's sign-in now returns to `?view=apps&manage=1`, which opens the drawer with
  "Account connected".
- **Manage** / **Reconnect** on an app opens the same drawer.

## Links that moved

- Settings no longer lists Connections. `?view=settings&section=connections` (old
  links, bookmarks) opens Apps with the drawer open.
- "Ask an admin to reconnect" in Activity and Help's "Connect your business tools" point
  to Apps.

## Checks

- PostgreSQL (`postgres/apps_tests.rs`): apps list actions with their kinds and rules
  (a delete blocked by default), agents and covering company rules; members don't see
  account names.
- Web (`tests/apps-page.test.mjs`): members see apps, actions, agents and rules
  read-only, and the "need you" filter; admins' old Connections link opens the drawer;
  Add app opens the connection controls. Navigation and Settings tests follow the move.
- Real Goose (`tests/workspace_connections.mjs`, `tests/connector_providers.mjs`,
  `tests/shared_agent_control.mjs`): the whole connection journey runs from Apps; a
  member sees the app read-only at desktop and 390 px with no horizontal overflow.

## Not yet

- Company systems (link-only apps, self-service MCP, API description) and built apps.
- People access per app and access requests.
- A Computers page in Settings (no computers feature exists yet).

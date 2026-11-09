# Apps page (first part)

**Apps** is in the rail between Library and Settings. It shows every service the company
connected, what agents may do with each action and which agents use it. Connections moved
here from Settings. From the Apps plan (`proj-docs/apps/`, AP1–AP2, draft decision D243).
Built 2026-11-05. Company systems (link apps), people access and requests were added
the same day (schema 75), and [built apps](built-apps.md) after them (schema 78).

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
`id, server, name, kind ("service"), account (admins only), status
("connected" | "reconnect"), actions [{id, title, meaning, mode}], agents [{id, name,
avatar}], rules`. Since schema 108 each also has `state` (`ok`, `reconnect`,
`writes_paused`, or `paused` while Cuckoo switched its provider off),
`paused_action` (the unconfirmed write's id when `writes_paused`), `connected`
(`{at, by_name}`, or `null`), `capabilities` (the provider's flags) and, for admins
only, `provider` and `via` ("Composio", "Pipedream" or "MCP server").

## Add app catalog and one-step connect (schema 108, server)

- `GET …/apps/catalog?query=&category=&cursor=` (any member): the catalog apps this
  workspace can connect, each through its routed provider. Search is on name and
  description; `category` is one of the catalog's eight. Connected apps come first,
  then by name, 30 a page with `next_cursor`. Each app has `id, name, description,
  category, kinds` (what its actions do, in Controls' meanings), `actions`, `state`
  (`connected`, `available`, or `requested` when the viewer asked for it) and
  `connection`. `categories` counts the matching apps per category.
- Members ask for a catalog app with the existing app requests:
  `POST …/apps/requests` with `{id, connector, reason}`; it reaches admins as a
  suggestion naming the app, once per member while it waits.
- `POST …/apps/connect` (admins) with `{app, request}`: approves the app and starts
  connecting it with Cuckoo's safe defaults (reads allowed, deletes blocked,
  everything else asks first). Answers `{redirect_url}` for the app's sign-in page or
  `{connection}` when verified at once; the same `request` returns the same answer,
  and a failed attempt can be retried with it. An abandoned sign-in leaves the app
  approved but not connected, which grants nothing.
- `GET …/control/available` and `GET …/agent-profiles/{a}/control`: each connection
  also has `state`.

The screens for these are separate work ([connector screens](../designs/connector-screens/README.md)).

## Your systems: company apps (schema 75)

A company app is a link to a system the company already runs (an order portal, a
delivery tracker). It shows as a card beside the services and opens **through Cuckoo**,
which checks who may open it first; the system keeps its own sign-in.

- **Filters**: All · Your systems · Services, beside the "need you" pill.
- **Who can open it**: everyone in the workspace, or chosen people; its **owner** and
  admins always can. Members see every company app; those who can't open one see it
  without its address, as "No access", with **Request access**.
- **Opening**: `GET /api/identity/workspaces/{w}/apps/systems/{id}/open` checks access,
  writes "Opened "Order portal"" to the Audit log and redirects (303) to the address;
  anyone else gets "not found". Addresses must be `https://`.
- **Admins** add, change and remove company apps (Add app → "Add your company's app";
  Manage; Remove) with `POST …/apps/systems` (`{id, name, description, url, audience,
  people, owner, revision}`; 0 adds, a stale revision is refused) and
  `POST …/apps/systems/{id}/remove`. Up to 200 per workspace.
- **Requests**: a member asks for access (`POST …/apps/requests` with `{id, app,
  reason}`; one waiting request per app, retries keep their id) or **suggests an app**
  (`{id, name, url, reason}`). The app's owner and admins see "N requests"
  (`GET …/apps/requests`) and approve or decline (`POST …/apps/requests/{id}`); the
  first decision stands. Approving access adds the person; suggestions go to admins.
  Every step is in the Audit log in plain words.

`GET …/apps` also returns `systems` (each with `can_open`, the address only when it can,
`audience`, `owner_name`, `people` for its owner and admins, `requested`) and
`requests_waiting` for the viewer.

## What admins do

- **Add app** offers "Add your company's app" or "Connect a service"; the latter opens a
  drawer with the existing connection controls: approve a system,
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

- PostgreSQL (`postgres/company_apps_tests.rs`): only admins add apps, https only, an add
  can't overwrite; a member without access sees no address and can't open; the owner
  opens and decides; one waiting request per app, retries safe; the first decision
  stands; approval opens it; suggestions go to admins only; removal closes it; every
  step is audited.
- Web (`tests/apps-page.test.mjs`): a member filters to Your systems, requests access
  once per click burst and suggests an app; people allowed open through Cuckoo in a new
  tab; admins decide requests and add a company app.
- Browser (`tests/company_apps.mjs`): an admin adds the portal for chosen people with an
  owner; a member asks for access (desktop, 390 px without overflow); the owner approves
  and Cuckoo then redirects the member to it; openings are audited; dark mode.

## Not yet

- Self-service MCP servers and API descriptions for company systems (agents using them).
- Built apps reading and writing data, the builder agent and manifest actions (see
  [built apps](built-apps.md)).
- A Computers page in Settings (no computers feature exists yet).

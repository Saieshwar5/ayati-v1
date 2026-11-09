# Apps page (first part)

**Apps** is in the rail between Library and Settings. It shows every service the company
connected, what agents may do with each action and which agents use it. Connections moved
here from Settings. From the Apps plan (`proj-docs/apps/`, AP1–AP2, draft decision D243).
Built 2026-11-05. Company systems (link apps), people access and requests were added
the same day (schema 75), and [built apps](built-apps.md) after them (schema 78). The
approved [connector screens](../designs/connector-screens/README.md) replaced the
"Add and manage apps" drawer on 2026-11-08: each service is managed in its own panel,
and Add app is one catalog for every way to connect.

## What everyone sees

- **Cards** for each approved, connected service: the app's first letter on a colour
  (stable by server id), its name, the account (admins only), the faces of the agents
  that use it and when it was last used ("Used 10 min ago"; "Connected today" before
  that). A tag names the one thing that needs attention: **Writes paused**,
  **Reconnect**, **Paused by Cuckoo** (its connection service is switched off for a
  while) or **Not used yet**.
- **Search** by name, and a **"N need you"** pill that shows only apps with a problem
  someone can fix (Reconnect, Writes paused).
- **The side panel** is the one place to manage a service: its health, what agents may
  do (each action with its kind and its Allow / Ask first / Block rule), which agents
  use it and how many company rules cover it, and **Account** (connected by and when,
  last used, whether sign-in works). Writes paused explains itself, with **Check what
  happened** opening "Did this go out?".
- Members read all of this and can't change it; they see **Request an app** instead of
  Add app. The server applies the same boundary.

`GET /api/identity/workspaces/{w}/apps` returns `{apps, can_manage}`; each app has
`id, server, name, kind ("service"), account (admins only), status
("connected" | "reconnect"), actions [{id, title, meaning, mode}], agents [{id, name,
avatar}], rules`. Since schema 108 each also has `state` (`ok`, `reconnect`,
`writes_paused`, or `paused` while Cuckoo switched its provider off),
`paused_action` (the unconfirmed write's id when `writes_paused`), `last_used` (`{at, agent_name}`, `at` in milliseconds, from schema 110), `connected`
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

The screens for these are described below and in [connector screens](../designs/connector-screens/README.md).

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
- **Admins** add, change and remove company apps (Add app → "Your company's app";
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

- **Rules and kinds in the panel.** Each action has the Allow / Ask first / Block picker
  (`POST …/control/connector-rules`). The kind under its name (▾) opens a small menu to
  correct what the action does (`POST …/control/action-meanings`; reading actions only
  read) and, for Ask first, to require someone other than the requester to approve.
  "· suggested" marks a kind Cuckoo guessed. The panel reads rules, revisions, the
  account's connection and the app's approval from `GET …/control/connections`, loaded
  beside `/apps` for admins; a refused save reloads them.
- **Reconnect** signs the same connection in again on the app's own page
  (`POST …/control/connections` with its id and revision). **Disconnect …** at the bottom
  asks in a dialog, then removes the app's approval (`POST …/control/systems`,
  `approved: false`), which also turns the account off and releases it at the provider;
  connecting again starts from safe rules. If the provider didn't confirm, Apps says to
  remove access there too. The connection service ("through Composio") is shown to
  admins only, in small text under Account.
- **Add app** is one dialog (`catalog-dialog.tsx`): services by category from
  `GET …/apps/catalog?query=&category=&cursor=` (searched by the server after a short
  pause in typing, each search aborting the last and late answers ignored; **Load more**
  follows `next_cursor`), tiles with the kinds of action and their state, then
  **Other ways to add**: MCP server (shown as next, not yet actionable), Your company's
  app and Build an app. A connected tile opens its panel. Members see **Request** on
  each tile, sent as an app suggestion under the app's name (`POST …/apps/requests`),
  and Suggest another app.
- **Connect** (`connect-dialog.tsx`) shows the starting rules first (Cuckoo's safe
  defaults: reading allowed, deleting blocked, the rest asks first; from the app's
  actions in `/control/connections`, else one row per kind), then sends one
  `POST …/apps/connect {app, request}`. Double clicks send once; a retry of the same app
  reuses `request`. `{redirect_url}` opens the app's sign-in page in this tab (the app is
  remembered in session storage for the return); `{connection}` shows the result at once.
- **Back from sign-in** (`?view=apps&manage=1&connected=1`): "Google Drive is connected",
  naming the account and its starting rules, with **Choose agents** (Agents),
  **Change rules** (the panel, focused on the rules) and Done. `connect_error=1` says the
  sign-in didn't finish and nothing was connected, with Try again. Both are shown once:
  the query is removed from the address.
- **Did this go out?** (`approvals/outcome-dialog.tsx`) loads the action details with
  `settle`, shows the exact fields, who asked and approved, what Cuckoo already checked,
  two answers and an optional note, then `POST …/control/actions/{id}/outcome`. A `409`
  says someone else answered first and shows their answer; any other failure keeps the
  answer and note. People without `can_settle` see who can answer. It opens from the
  panel, from **Check** in Activity's Needs review and from the agent's Tools tab.

Where agents get their tools ([agent setup](agents-team-workspace.md) step 2 and the Tools
tab), each app shows the same health tag (from `state` on `/control/available` and the
agent's control), with the fix where there is one: **Check it** for writes paused,
**Reconnect on Apps** for admins. **Need another app?** sends members' requests through
the existing app suggestions; admins connect one there, with sign-in in a new tab so a
draft agent is kept, and the list reloads on Done.

## Links that moved

- Settings no longer lists Connections. `?view=settings&section=connections` (old
  links, bookmarks) and `?view=apps&manage=1` ("Ask an admin to reconnect" in Activity)
  open Apps on the first app that needs attention.
- Help's "Connect your business tools" points to Apps; the agent setup and Tools tab
  lines no longer mention Settings → Connections.

## Checks

- PostgreSQL (`postgres/apps_tests.rs`): apps list actions with their kinds and rules
  (a delete blocked by default), agents and covering company rules; members don't see
  account names.
- Web (`tests/apps-page.test.mjs`): members see apps, actions, agents and rules
  read-only, and the "need you" filter; old Connections links open Apps on the app that
  needs attention; Add app opens the catalog. `tests/apps-service-panel.test.mjs`: an
  admin's rule saves once per click burst and a refused save says why; the kind menu
  (keyboard, Escape) saves a kind and a second approver; disconnect asks in the page and
  sends once; reconnect reuses the connection; card tags, last use and account facts;
  members read the same panel and the client falls back to `status`.
  `tests/apps-catalog.test.mjs`: search waits for a pause and ignores a stale answer;
  Load more asks once; a category starts over and drops a page for the old one; members
  request. `tests/apps-connect.test.mjs`: rules preview, one request per click burst,
  retry with the same `request`, redirect, the connected and failed returns.
  `tests/outcome-dialog.test.mjs`: both answers, `409`, a failed save keeps the note,
  read-only without `can_settle`, Check from Activity. `tests/agent-app-health.test.mjs`:
  health tags on the Tools step and tab, a member's request from setup, an admin
  connecting in a new tab while the draft is kept.
- Browser (Vite with route fixtures, 2026-11-08): the screens in
  `designs/connector-screens/built-*.png` at 1440 × 900 in light and dark and the panel
  at 390 × 844; the kind menu, rule save, disconnect dialog, catalog search, paging and
  categories, connect to the sign-in page, members' view, 320 px and short windows. No
  server endpoints from schemas 107–108 were exercised.
- Real Goose (`tests/workspace_connections.mjs`, `tests/connector_providers.mjs`,
  `tests/shared_agent_control.mjs`) still drive the removed drawer ("Approve system",
  "Connect account", "Safety for …") and need moving to the panel and one-step connect
  once `/apps/connect` exists.

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

- Self-service MCP servers (shown in Add app as next) and API descriptions for company
  systems (agents using them).
- Built apps reading and writing data, the builder agent and manifest actions (see
  [built apps](built-apps.md)).
- A Computers page in Settings (no computers feature exists yet).

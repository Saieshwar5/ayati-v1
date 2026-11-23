# Apps an admin adds from the library (schema 135)

Built 2026-11-19; run locally in a browser against a stand-in for Composio,
not yet in the cloud or with a real account. Cuckoo ships with
ten built-in apps ([its catalog](connector-providers.md#cuckoos-catalog)). A
workspace's admins can add more from the provider's library (Composio's, about
1,800 apps), choosing which actions agents get. An added app belongs to that
workspace only and then works like a built-in app: it is
[allowed](allowed-apps.md), members connect it to their agents, reads are
allowed and changes ask first.

## What the admin does

**Settings › Apps › Add apps.**

1. **Search the library.** Each result is one of:
   - *addable*: Composio signs the app in itself (managed OAuth);
   - *already one of Cuckoo's apps*: a built-in app, or one already added;
   - *not supported yet*: it needs a pasted API key or the company's own
     sign-in setup, or has no account to connect.
2. **Choose actions**, at most 20 an app, a page of 30 at a time with a search.
   - Each chosen action is **Reads** or **Changes · asks first**. Cuckoo
     pre-fills it from Composio's hints: a read-only hint is a read; anything
     else asks first.
   - An action Composio marks as creating, updating or destroying can't be
     marked as reading. One Composio says nothing about is the admin's call.
   - An action is *not available* when it requires an input Cuckoo doesn't
     offer (see the limits), or Composio is retiring it.
3. **Add.** The app is saved and allowed for the workspace at once.

Afterwards the app's row in Allowed apps says "Added from the library" and has
**Change actions** and **Remove** beside Allow and Switch off.

- **Changing the actions ends every connection of the app.** A saved connection
  carries the review it was made under, so its owners connect again to get the
  new one. The dialog says how many connections end before saving.
- **Removing** ends every connection, hands each account back to the provider
  to revoke, declines waiting requests for it and stops offering the app.
  Adding it again starts a new review.

## How it works

- **The review is read on the server.** The browser sends only the library
  app's name, the chosen tools and each one's effect. Names, descriptions and
  fields are read from Composio when saving, and each tool must belong to that
  app. An effect the hints forbid is refused.
- **Ids.** The app's id is its library name in lower case (`hubspot`); an
  action's is the app, then the tool without the app's prefix
  (`HUBSPOT_CREATE_NOTE` is `hubspot.create_note`). Titles and field names are
  Composio's own.
- **Fields.** Up to 20 of a tool's plain fields and lists of plain values,
  required ones first: the same rule as built-in apps
  ([the catalog](connector-providers.md#cuckoos-catalog)).
- **Storage.** `relay_added_apps` holds one row per workspace and app: the
  library name and the reviewed actions (`id`, `title`, `effect`, `tool`,
  `fields`). At most 30 apps a workspace. The allowed state, requests,
  connections, rules and receipts use the same tables as built-in apps.
- **Providers.** The provider is built for a workspace with its added apps when
  listing apps, connecting one and finishing a sign-in
  (`connectors::apps`, `provider_for`, `named_for`). A saved connection needs
  none of that: its binding names the toolkit and each saved operation's schema
  lists the reviewed fields, so calls, the drift check and any controller
  process work from the connection alone.
- **Changes at Composio.** As for built-in apps, the tool version is pinned and
  checked again before every call; a difference refuses the call as changed
  and nothing is sent.
- **A built-in app wins.** If a later release ships an app with the same id,
  the workspace's added row is ignored and its connections are refused as
  changed until their owners connect the built-in app.

## API (admins)

| Endpoint | Purpose |
| --- | --- |
| `GET …/apps/library?q=` | `{apps: [{app, id, name, description, actions, state, reason}]}`; `state` is `addable`, `builtin`, `added` or `unsupported` |
| `GET …/apps/library/{app}/actions?q=&page=` | `{app, id, name, total, actions: [{tool, title, description, effect, may_read, available, reason}], next, added}` |
| `POST …/apps/added` | `{app, actions: [{tool, effect}], revision}`; revision 0 adds, otherwise changes. Answers `{saved, app, name, ended}` |
| `POST …/apps/added/{id}/remove` | `{revision}`; answers `{removed, ended}` |
| `GET …/apps/allowed` | each app also carries `added` (`{revision, toolkit, actions, added: {at, by_name}}` or null), and the answer says `library` (whether a library is configured) |

Audit log: `added_app_saved`, `added_app_changed`, `added_app_removed`.

## Checks

- Unit: `connectors::added` (offered fields, ids, categories, the review a
  saved operation carries) and `connectors::composio_library` (hints,
  which apps can be added).
- PostgreSQL `allowed_apps::added::tests`, against the local Composio
  stand-in: the library lists apps and pages of actions and builds only what
  was reviewed; an added app is allowed, connected by a member and run through
  a provider that doesn't know the workspace's list; a changed tool and a
  connection without its toolkit are refused; changing or removing ends both
  agents' connections; stale revisions and members are refused; another
  workspace never sees the app.
- Web `tests/added-apps.test.mjs`: search on submit only; unsupported and
  built-in apps can't be chosen; an unavailable action and a forbidden "Reads";
  paging; a failed save keeps the choices and a double click saves once;
  changing with its warning; removing after confirming; no library.
- Process `tests/added_apps.mjs`, a real controller and a real browser against
  the stand-in: search, review and add HubSpot; connect it on an agent through
  the sign-in page (the read allowed, the change asking first); change its
  actions, which ends and revokes the connection; remove it. Run with a
  `relay_simulation_*` test database: `node --test tests/added_apps.mjs`.
- Against Composio itself on 2026-11-19, read-only: searching, listing
  HubSpot, Salesforce and Asana, and building HubSpot with three actions.

## Limits, and not verified

- **No account of an added app has been connected for real**: sign-in and
  calls ran only against the stand-in.
- **About half of a large app's actions are unavailable** (42 of HubSpot's
  first 90, 67 of Salesforce's, 63 of Asana's were available). The usual cause
  is a required input that is a nested object, a list of records, or accepts
  two types (HubSpot's "Create note" requires a timestamp given as text or a
  number). Offering fields that accept two plain types would be the next step.
- **Only apps Composio signs in itself.** Apps that need a pasted API key or
  your own OAuth app can't be added (Trello, for example).
- **No events** for added apps, and no identity read, so reconnecting an
  account starts its rules over.
- **Names are Composio's own**: action titles and field names are not rewritten.
- **Searching is Composio's**: it matches loosely and lists alphabetically.
- **Every change ends the app's connections**, also when it only adds an action.
- A member can't ask for an app that isn't offered; they ask an admin outside
  Cuckoo.

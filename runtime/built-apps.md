# Built apps (first part)

People build small apps for their team inside Cuckoo — a leave request form, a stock
counter — and publish them on the [Apps page](apps-page.md). From the Apps plan
(AP4–AP9). Built 2026-11-05: the code store, the checks, versions and a safe runner
(schema 78), the app's own data and roles through a message bridge (schema 79), and
agents that write the draft for the builder who asks (schema 80), and agents acting in
apps for a person, under per-action rules, with Leave, Expenses and Stock templates
(schema 81), and app events that start agent work (schema 84).
Apps can't read or change other Cuckoo data.

## How an app is kept

- An app is a set of text files: `cuckoo-app.json` (name, description, roles,
  collections, actions), `index.html`, its styles and scripts, and a README. Every app
  starts from that starter and passes its checks as it is.
- Files live in PostgreSQL (`relay_built_app_files`), one row per file and version.
  **Version 0 is the draft**; each publish copies the draft to the next numbered version
  (`relay_built_app_versions`, with a note, who and when). `relay_built_apps.published`
  names the version people open; none means not published, or taken offline.
- Up to 200 files, 256 KiB each, 2 MiB in all; 100 apps per workspace. Paths are plain
  names and folders: no hidden files, no `..`.

## Who does what

- **Anyone** in the workspace starts an app and becomes its **builder**. Repeating the
  same create (same id) is harmless.
- **The builder** changes the draft, one file at a time, from the draft revision they
  read; a stale write is refused (409) and the browser keeps the edit. Only the builder
  publishes.
- **The builder and admins** see the code and versions, preview the draft, download any
  version as a ZIP, change the name and who can use it, make an earlier version live,
  take the app offline, or remove it.
- **People** open the published version if it is for everyone or they were chosen.
  Drafts and unpublished apps are invisible to them. Code and versions are never sent to
  them. Starting, publishing, switching versions, settings, removal and every opening
  are audited.

## Checks before publishing

Run by the server on the saved draft when listing it and again on publish; any failure
blocks publishing and says why in plain words:

- **manifest**: `cuckoo-app.json` exists, is JSON, has a name, roles, and valid
  collections and access (below);
- **screens**: `index.html` exists;
- **size**: the limits above;
- **secrets**: private keys, cloud access keys and common API token shapes;
- **libraries**: HTML, CSS or JavaScript that loads anything from the internet — add the
  file to the app instead.

## The manifest: roles, collections and access

```json
{
  "name": "Leave",
  "roles": ["user", "approver"],
  "collections": {
    "requests": {
      "fields": { "start": "date", "days": "number", "reason": "text", "approved": "boolean" },
      "access": { "user": ["create", "read_own", "update_own"], "approver": ["read", "update"] }
    }
  }
}
```

- **Roles**: one to ten names. Everyone who may open the app holds the **first** role;
  the builder and admins give the others to chosen people (Apps → the app → **Roles**).
- **Collections**: up to 20, each with 1–40 fields typed `text` (up to 5,000
  characters), `number`, `date` (`2026-11-05`) or `boolean`.
- **Access** per role: `create`, `read`, `update`, `delete` for any record, or
  `read_own`, `update_own`, `delete_own` for records the person created. A role not
  listed can do nothing with that collection.
- The manifest check reports the first problem in plain words (an unknown role in
  `access`, a field type, a permission name), so a broken manifest can't be published.

## The app's data

- Records live in `relay_built_app_records`: an object of the collection's fields, a
  revision, who created and last changed it and when. Up to 5,000 records per app and
  16 KiB per record. Unknown fields and wrong types are refused.
- The **published** app reads and writes `live` records, by the published version's
  manifest and the person's roles. The **draft preview** (builder and admins) uses the
  draft's manifest, every role, and separate `preview` records, so trying a draft never
  touches the data people rely on. An app taken offline can't reach its data.
- Updates name the revision read and merge the fields sent; a newer revision is refused
  (409). Lists return up to 200 records, newest first, with `mine` and the creator's name.

## The bridge

The runnable document gets a small script first (`built_apps/bridge.js`) that defines
`window.cuckoo`:

```js
await cuckoo.me();                         // { id, name, roles }
await cuckoo.list('requests', { mine });   // { records: [{ id, data, revision, mine, by, … }] }
await cuckoo.get('requests', id);
await cuckoo.create('requests', { days: 2 });
await cuckoo.update('requests', id, { approved: true }, revision);
await cuckoo.remove('requests', id);
```

Each call posts a message to the Apps page. The page answers only messages whose
source is that app's own frame, allows eight waiting at once, and relays each one to
`POST …/apps/built/{id}/data`, where the server makes every decision. Answers go back
by message; failures reject the promise with the server's plain-words reason.

## Templates

**Build an app → Start from** offers a blank page or a template: **Leave requests**,
**Expenses** or **Stock** (`built_apps/templates/`). Each is a manifest with roles,
a collection and agent actions, plus `config.js` (collection, fields, the decision field
and who decides) read by one shared `records.js` screen: a form to add a record, the
records the person may see, and Approve/Decline for roles that may change any record.
Every template passes its checks as it is, so it can be published at once.

## Agents acting in apps

- The manifest's `actions` list what agents may be allowed to do:

  ```json
  "actions": [
    { "id": "file_leave", "title": "File a leave request", "collection": "requests",
      "does": "create", "fields": ["start", "days", "reason"] },
    { "id": "my_leave", "title": "See leave requests", "collection": "requests", "does": "read" },
    { "id": "decide_leave", "title": "Approve or decline a leave request",
      "collection": "requests", "does": "update", "fields": ["status"] }
  ]
  ```

  `does` is `create`, `update` or `read`; create and update name the fields an agent may
  set. Up to 20 actions; the manifest check rejects unknown collections and fields.
- **Every action starts blocked.** On the app's panel, **Agents can** lists the actions
  for everyone who can see the app; its builder or an admin sets each to **Allow**,
  **Ask first** or **Block** (`relay_built_app_agent_rules`, schema 85 adds ask), audited.
- **Ask first**: `run_app_action` answers `needs_confirmation` with a question Cuckoo
  wrote — "May Nia file a leave request in Leave?" with the fields, Yes/No — and a key
  derived from the exact fields. The agent puts it to the person with `ask_question`;
  when they answer, the same request runs the action again and it goes ahead only if
  that exact question was answered **yes** by the person the agent works for. A changed
  question, a "no" or a decline refuses it. `app_actions` marks such actions
  `asks_first`.
- An agent gets `app_actions` (what it may do, per app the person it works for can open)
  and `run_app_action` (`{app, action, data, record, revision}`), and an instructions
  line when any action is allowed. It **acts as that person**: the same checks as their
  own requests, by the published manifest and their roles, so it can never do more than
  they could. Fields beyond the action's are refused; a stale revision returns
  `changed`.
- A run is kept by a one-time key (the request plus its exact arguments,
  `relay_built_app_action_runs`); repeating it returns the first result. Records keep
  the agent that made or last changed them (`by_agent`, shown as `agent`). Creates and
  updates are audited: "Nia did \"File a leave request\" in \"Leave\" for Mina".

## App events

- On a published app's panel, **When something is added** lists its collections. The
  builder or an admin chooses **Ask an agent** for one: an agent and what it should do
  with each new record (`GET/POST …/apps/built/{id}/watchers` with `{collection, agent,
  instructions}`, or `agent: null` to stop). Cuckoo opens a conversation "<app> (app)"
  between that person and the agent; starting it also checks they may work with the
  agent. Setting and stopping are audited.
- Each record added to that collection in the **live** app is queued once
  (`relay_built_app_events`); preview records aren't, and neither are records the
  watching agent added itself, so it can't loop. The scheduler starts one at a time per
  collection, as background work for the watcher in their conversation, with the
  record's fields, who added it (and through which agent) and the instructions. The
  agent acts **as the watcher**: in the app it can do only what they could and what
  Agents can allows. Paused agents, an app taken offline or a removed record mark it not
  handled, with a reason; stopping skips what waits.

## Building with an agent

- On the app's panel the builder chooses **Build with an agent**, an agent and what to
  build. Cuckoo records that this agent may work on this app
  (`relay_built_app_helpers`), then opens the builder's direct chat with the agent with
  the request in the message box, unsent. The person sends it.
- In a request **from the app's builder**, the agent gets four ayati_control tools and a
  short guide to apps (`built_apps::files::BUILDER_GUIDE`: files, manifest, the
  `cuckoo` bridge, no internet, no keys):
  - `my_apps` — the apps this person asked it to build;
  - `app_files` — a draft's files and sizes, its revision and what the checks find;
  - `read_app_file` — one draft file;
  - `write_app_file` — one whole file from the draft revision it read; a stale
    revision returns `changed`, and the result carries the checks.
- In anyone else's request, the same agent gets `not_allowed` for that app. It never
  publishes: the builder previews and publishes as with their own changes. Every file it
  writes is audited ("Juno changed app.js in \"Stock count\""), and asking or stopping an
  agent is audited too.
- The panel lists **Agents building it**; the builder can **Stop** each one, which closes
  the draft to it again.

## How an app runs

`GET …/apps/built/{id}/run` (or `?preview=true` for the draft) returns the app as one
document: local styles and scripts inlined. The Apps page loads it in an iframe with
`sandbox="allow-scripts allow-forms"` and no `allow-same-origin`. The response itself
carries `Content-Security-Policy: sandbox allow-scripts allow-forms; default-src 'none';
script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; font-src data:;
connect-src 'none'; form-action 'none'; base-uri 'none'; frame-ancestors 'self'`, and the
document repeats the policy in a meta tag. So the app runs in a unique origin: it can't
read Cuckoo's cookie, storage or page, and every network request it makes is blocked.

A `srcdoc` frame would inherit the Apps page's own policy (`script-src 'self'`), which
blocks inline scripts; serving the document from its own route gives it its own policy.

## Routes

All under `/api/identity/workspaces/{w}/apps`:

| Route | Who | Does |
| --- | --- | --- |
| `GET` (list) | everyone | `built` cards beside services and systems |
| `POST /built` `{id, name, description, audience, people}` | anyone | starts from the starter |
| `POST /built/{id}` (same body) | builder, admins | name, description, who can use it |
| `GET /built/{id}/files?version=` | builder, admins | files, draft revision, checks |
| `POST /built/{id}/files` `{path, content \| null, revision}` | builder | writes or deletes a draft file |
| `POST /built/{id}/publish` `{note, revision}` | builder | `checks_failed` with the checks, or `published` with the version |
| `POST /built/{id}/rollback` `{note, revision: version}` | builder, admins | that version goes live; 0 takes it offline |
| `POST /built/{id}/remove` | builder, admins | hides it from everyone |
| `GET /built/{id}/run[?preview=true]` | people allowed; draft for builder, admins | the sandboxed document |
| `GET /built/{id}/download?version=` | builder, admins | ZIP of that version |
| `POST /built/{id}/data` `{preview, op, collection, record, data, revision, mine}` | people allowed; preview for builder, admins | one bridge request: `me`, `list`, `get`, `create`, `update`, `delete` |
| `GET /built/{id}/roles` | builder, admins | the roles and who holds them |
| `POST /built/{id}/roles` `{role, people}` | builder, admins | who holds a role (not the first); audited |
| `POST /built/{id}/helpers` `{agent, helps}` | builder | asks an agent to build the draft, or stops it; audited |
| `GET /built/{id}/agent-rules` | anyone who can see it | each manifest action and whether agents may run it |
| `POST /built/{id}/agent-rules` `{action, mode}` | builder, admins | allow or block one action for agents; audited |
| `GET /built/{id}/watchers` | anyone who can see it | each collection and the agent acting on new records |
| `POST /built/{id}/watchers` `{collection, agent, instructions}` | builder, admins | ask an agent to act on new records, or stop it; audited |

## In the browser

Apps gains a **Built here** filter and a **Build an app** button. A built card shows
**Draft** or **Offline** to its builder and admins. Its panel opens the app
(`?view=apps&run={id}`), and for the builder and admins offers **Code and publish** (or
**Code and versions** for admins), **Preview draft**, **Roles**, **Settings** and
**Remove**. The
code drawer lists files, edits one at a time with save and discard, adds and deletes
files, shows the checks, publishes with an optional note, and lists versions to make live
or download.

## Checks run

- Unit (`built_apps/files.rs`): the starter passes its checks and runs as one document
  with the blocking policy; paths reject hidden and parent parts; checks name a missing
  page, a token and an internet script.
- PostgreSQL (`postgres/built_apps_tests.rs`): drafts are invisible to others; only the
  builder writes, from the current revision; a key blocks publishing; a chosen person
  opens the published version only; draft edits stay out until the next publish; admins
  make an earlier version live and take it offline; settings, ZIP download and removal.
- Web (`tests/built-apps.test.mjs`): building sends one create per click burst and shows
  the draft; a stale save keeps the edit; failing checks are shown and success goes live;
  the runner frame has no `allow-same-origin` and loads the run route; people without
  access see why; others see no code controls.
- Browser (`tests/built_apps.mjs`, real Chromium and server): a member builds an app for a
  colleague, a key blocks publishing, version 1 publishes and downloads as a ZIP; the
  colleague opens it and the app itself reports network, parent page and cookie all
  blocked (desktop and 390 px); an admin takes it offline and the colleague can't open
  it; the steps are audited; dark mode.

- Unit (`built_apps/manifest.rs`): own-only and full permissions per role; field types
  and dates; a broken manifest says what is wrong. The bridge is injected before the
  app's own code.
- PostgreSQL (`postgres/built_app_data_tests.rs`): no live data before publishing; the
  preview has every role and its records stay apart; members create and see only their
  own; fields enforced; nobody deletes without the permission; only the builder or an
  admin gives roles, never the first; approvers read and update all; stale revisions
  refused; unknown collections and operations refused; offline apps reach no data.
- Web (`tests/built-apps.test.mjs`): the runner relays bridge messages from its own frame
  only and answers successes and refusals; the Roles dialog shows holders and saves.
- Browser (`tests/built_app_data.mjs`): a leave app built through the API; the builder's
  preview shows every role; two members ask for leave and each sees only their own; the
  builder makes a colleague an approver in the Roles dialog; she sees every request and
  approves one, which the member then sees; a member can't reach preview data; audited.
- PostgreSQL (`postgres/built_app_agents_tests.rs`): before the builder asks, the agent
  sees no apps; only the builder asks (not an admin); the agent lists, reads and writes
  the draft from the current revision, a stale write is refused, a key shows in the
  checks, bad paths fail; in someone else's request it can't reach the app; stopping it
  closes the draft; writes and asking are audited.
- Web (`tests/built-apps.test.mjs`): Build with an agent records the agent and opens the
  direct chat with the request waiting unsent; the panel lists agents and stops one.
- Real Goose (`tests/built_app_agent.mjs`): a member starts an app and asks Juno from
  it; the chat opens with the request ready; once sent, Juno (scripted model, real
  Goose and ayati_control) writes the manifest, page and script; the checks pass and it
  doesn't publish; the guide and tools reach the model; the member previews the draft,
  which keeps a count through the bridge, publishes it and stops Juno.
- Unit (`built_apps/actions.rs`, `built_apps/templates.rs`): actions name a collection
  and its fields and say what is wrong; every template passes its checks, offers agent
  actions and escapes its name.
- PostgreSQL (`postgres/built_app_actions_tests.rs`), on the Leave template: every action
  starts blocked; members see the rules, only the builder or an admin changes them;
  once allowed the agent files leave as the person, the record names the agent, a retry
  doesn't repeat it; extra fields, an unallowed action and a bad date are refused; even
  allowed, approving needs the approver role the person lacks; blocking again closes it;
  runs and rule changes are audited.
- Web (`tests/built-apps.test.mjs`): Start from sends the template; Agents can shows each
  action's mode and the builder changes one; people who only use the app see it
  read-only.
- Real Goose (`tests/built_app_actions.mjs`): a member starts Leave from its template,
  publishes it and allows only filing; a colleague asks Nia in Chat; Nia sees only
  "file leave", files it as her (recorded with Nia), and approving is refused; the
  colleague opens Leave and sees the request waiting; audited.
- PostgreSQL (`postgres/built_app_events_tests.rs`): only after publishing, only its
  builder or an admin, only declared collections; a colleague's live request is queued
  once and starts work for the watcher in their conversation, with who added it and its
  fields; a preview record and the agent's own record start nothing; the next waits its
  turn; the agent acts with the watcher's roles; stopping skips what waits; audited.
- Web (`tests/built-apps.test.mjs`): asking an agent and stopping it from the panel, the
  conversation link for the watcher only, read-only for people who use the app.
- Real Goose (`tests/built_app_events.mjs`): the builder asks Nia from the panel in the
  browser; a colleague asks for leave in the running app; Nia, working for the builder,
  replies in their conversation and opens a task to decide; audited.
- PostgreSQL (`built_app_actions_tests.rs`, ask first): Cuckoo hands back its question;
  a different question under the key, even answered yes, refuses; the exact question
  answered yes runs it; a no refuses it.
- Real Goose (`tests/built_app_ask_first.mjs`): asked in Chat, Nia is handed Cuckoo's
  question and asks Mina; nothing is filed until Mina answers yes in the browser; then
  the same request files it; audited.

## Not yet

- The agent seeing what a preview shows or the errors it raises.
- Company rules from the Guard on app actions; events for changed records ("request
  approved").
- Agent actions declared in the manifest and run under company rules.
- Templates beyond the starter, and a Git-backed code store.

# Built apps (first part)

People build small apps for their team inside Cuckoo — a leave request form, a stock
counter — and publish them on the [Apps page](apps-page.md). From the Apps plan
(AP4–AP9). Built 2026-11-05: the code store, the checks, versions and a safe runner
(schema 78), then the app's own data and roles through a message bridge (schema 79).
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

## Not yet

- A builder agent that writes the code from a description (BA3).
- Agent actions declared in the manifest and run under company rules.
- Templates beyond the starter, and a Git-backed code store.

# Built apps (first part)

People build small apps for their team inside Cuckoo — a leave request form, a stock
counter — and publish them on the [Apps page](apps-page.md). From the Apps plan
(AP4–AP9). Built 2026-11-05 (schema 78). This first part is the code store, the checks,
versions and a safe runner; apps can't read or write Cuckoo data yet.

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

- **manifest**: `cuckoo-app.json` exists, is JSON, has a name and at least one role;
- **screens**: `index.html` exists;
- **size**: the limits above;
- **secrets**: private keys, cloud access keys and common API token shapes;
- **libraries**: HTML, CSS or JavaScript that loads anything from the internet — add the
  file to the app instead.

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

## In the browser

Apps gains a **Built here** filter and a **Build an app** button. A built card shows
**Draft** or **Offline** to its builder and admins. Its panel opens the app
(`?view=apps&run={id}`), and for the builder and admins offers **Code and publish** (or
**Code and versions** for admins), **Preview draft**, **Settings** and **Remove**. The
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

## Not yet

- Apps reading and writing their own data and roles through a message bridge (BA2).
- A builder agent that writes the code from a description (BA3).
- Agent actions declared in the manifest and run under company rules.
- Templates beyond the starter, and a Git-backed code store.

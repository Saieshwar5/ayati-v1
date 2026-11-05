# Workspace Library

First implementation, 2026-10-22, on `feat/workspace-library`. The editable
[OpenPencil concept](../designs/library/README.md) remains the broader design
reference. This increment delivers the deliberately smaller first release.

## Available behavior

Library is a React feature in the existing workspace shell. PostgreSQL workspaces,
including solo workspaces, support native pages, private/selected/workspace sharing,
file uploads, protected downloads, stars, search, trash/restore and page history.

Pages use headings, paragraphs, lists, checklists, links and basic formatting.
Tiptap owns the editor DOM. Structured content is validated by Rust before storage;
unsupported nodes/attributes, unsafe links, excessive depth and oversized content
are rejected. Page content is limited to 256 KiB and 5,000 nodes. Changes coalesce
into autosaves; the server checks the expected revision inside a workspace
transaction. The editor and agent handoff share acknowledged save state, so asking
waits for saving and uses the revision actually acknowledged by the server. Failed drafts remain browser-local, scoped by account/workspace/item.
Conflicts require explicit recovery rather than overwriting newer content.
Private browser drafts cannot be remotely erased while a device is offline.

Saving uses payload-bound request identities. Exact retries acknowledge the
original revision; another edit does not turn a lost response into another write.
Request records hold a payload fingerprint and a small acknowledgement, not an
extra copy of every document. Keep the latest 50 page revisions plus versions
referenced by Library Chat requests. History supports inspection and restore;
restore creates a new revision. Native pages export structured JSON.

List/search filter current access before pagination. Results contain at most 50
items per page. Search matches titles and prepared text, including supported file
excerpts. Stars are personal. The first release limits a workspace to 1,000 items,
including retained trash. Trash remains recoverable and is not automatically
purged. File bytes in trash continue to count toward the 1 GiB file quota.
Native page/history database storage is separate from that binary-file quota.

## People, ownership and removal

Admin/User remain workspace membership roles. Both can create Library items.
Only admins invite or remove workspace members through Settings.

- New items are private. The creator manages sharing and trash.
- Native pages offer Can view or Can edit. Edit does not grant resharing. The
  initial file-sharing UI offers view/download access; file replacement is deferred.
- Workspace visibility gives current members view access; named editors can edit.
- Another admin does not automatically read private/selected-people content.
- Links do not grant access. There is no guest/public-link sharing.
- Version history requires edit access; a viewer can fetch the current page version.

Sharing changes use a separate access revision and atomic grant replacement.
Downloads, search, history, agent retrieval and source-result publication recheck
access on the server. Fresh HEAD/range requests follow the same file rules.
Protected responses are not stored in the browser HTTP cache. Removing access
cannot recall an already downloaded copy or delivered private Chat answer.

Membership removal deletes human grants, stars, upload sessions and request
receipts through foreign keys. It clears current item stewardship while retaining
the original author/history. Re-invitation does not revive private ownership or
old grants. An admin can explicitly take over a workspace-visible orphaned item;
an existing editor can take over a selected-people orphaned item. This does not
grant an admin new read access to selected/private content. Private orphaned items
remain retained and inaccessible; broader recovery/deletion policy is still a
pilot decision. Selected items without surviving editors stay read-only.

## Files and storage configuration

Without a configured file backend, pages work and Upload explains that storage is
unavailable. Configuration is server-side, loaded at startup. Choose one backend:

```sh
# Private S3-compatible storage. Authentication stays in the server environment
# or the credential sources supported by object_store; never put keys in the UI.
CUCKOO_LIBRARY_S3_BUCKET=your-private-bucket
AWS_REGION=your-region
# Optional provider endpoint; HTTPS required except loopback test endpoints.
# CUCKOO_LIBRARY_S3_ENDPOINT=https://your-provider-endpoint

# Explicit local development alternative. Do not set together with S3_BUCKET.
# CUCKOO_LIBRARY_LOCAL_ROOT=/absolute/private/development/library-files
```

Standard AWS credential variables, including temporary session credentials, are
read by the storage SDK when configured. Never commit them. Configure the bucket
privately and test the chosen provider's conditional-create behavior. This work
does not provision a bucket or alter public-access/bucket policies. A local S3
emulator round trip does not establish a real provider/region deployment.

Uploads are limited to 25 MiB per file, with four active server transfers. This
first version uses authenticated same-origin transfers through Cuckoo rather than
browser-to-S3 signing or multipart upload. The queue survives same-workspace
feature navigation. Reload recovery reuses upload metadata and asks for the source
file again; size and SHA-256 must match before resuming. Browser closure does not
continue a transfer in the background. Selection is limited to ten files at once.

The server reserves quota transactionally for 24-hour upload sessions, verifies
size/checksum, writes a server-generated immutable object key, verifies stored
bytes, and rechecks authorization before publishing the item. Same filenames use
different item IDs. Exact completion retries reconcile an uncertain response.
Cancellation reports if a file was already stored. Local objects publish by an
atomic hard link after fsync; S3 uses conditional creation. Downloads verify the
stored checksum and recheck access after the storage read. Transfer timeouts are
bounded; the size limit bounds buffered downloads.

Reservations expire, but stored orphan bytes from a failed/cancelled operation are
retained for reconciliation. Automatic orphan deletion, permanent trash deletion,
file replacement/versioning and large multipart transfers remain follow-up work.
Do not enable a bucket lifecycle rule that can expire live Library objects.

PNG/JPEG originals can preview; PDF.js renders PDFs in a separate lazy worker
through an authenticated endpoint. Other formats remain downloadable. UTF-8 text
can prepare an agent excerpt. PDF text extraction is optional and requires Linux
`bwrap`, `prlimit` and `pdftotext`. It runs without network or home-directory access,
with limited memory, CPU, output size, pages and elapsed time. If isolation/parser
setup fails, the file remains stored/downloadable with extraction unavailable.
PDF excerpts cover at most 50 pages; each question uses limited prepared text.
Image OCR, audio/video transcription and arbitrary uploaded HTML/SVG rendering
are not included.

## Folders and agents answering from them (schema 69, LB1)

Built 2026-11-05 from the Library plan (slice LB1, draft decision D242).

**Folders.** One level: a folder holds pages and files, not other folders. Anyone can
create one (`POST /api/identity/workspaces/{w}/library-folders` with `{id, request,
title}`); it is private to its creator until shared. Everything in a folder follows the
folder's sharing: people (Only me, Selected people, Workspace; Can view or Can edit) and
agents. An item's own steward keeps access to it. Library → All shows one level, folders
first with their item count; `?folder=` lists one folder's items; other views and
searches show items wherever they are filed, with the folder's sharing and name.

- People who can edit a folder can create pages in it (`folder` on the first save) and
  file their own items into it. **Move** (`POST …/library/{id}/move` with `{request,
  revision: access_revision, folder | null}`) is for the item's creator. Moving in clears
  the item's own sharing; moving out makes it private again. Both are audited in plain
  words and fence work that used the item.
- An item in a folder can't be shared on its own ("share the folder or move the item
  out"). Sharing a folder raises the access revision of every item in it, so source
  receipts see the change.
- A folder goes to Trash only when empty. An item restored from a folder that is in
  Trash comes back unfiled and private.

**Agent access per folder or item**, set by whoever manages its sharing, separately for
each agent (`relay_library_agents.level`, `use_for`):

| Use for | The agent may use it when… |
| --- | --- |
| **People who can see it** (default) | everyone who will read the answer can open it: the asker, everyone in the conversation (every member for a public channel) and a schedule's posting conversation |
| **All staff** | any colleague asks, without them opening the document |

Customer answers ("Customers") wait for customer mode; replies that go outside the
workspace through the communication gateway can use neither. The only level offered is
**Read**: Suggest, Edit with approval, Edit and Save here are reserved for agent editing
(LB3) and refused until then. The server applies all of this (`relay_library_agent_use`),
never the agent. Sharing changes are audited with each agent's audience ("Changed sharing
of "HR": Nova reads it for people who can see it").

**Agent tools** (ayati_control, pooled executors):

- `list_folder({folder?})`: the folders and unfiled documents this agent may use here, or
  one folder's documents.
- `search_library({query})`: up to 10 documents matching up to eight words (a title match
  counts three times the text), with a short preview. This is a plain word match over the
  latest 200 usable documents; meaning-based search, summaries and more file types are LB2.
- `read_document({id, offset?})`: 6,000 characters at a time. Anything the agent can't use
  for these people is "not found", without saying whether it exists.

Each read leaves a receipt (`relay_library_reads`: reply, item, revision, access
revision). **Before the reply is published**, every read document must still be usable
for that audience with unchanged sharing, or the reply is withheld ("Request cancelled
because access changed"), like Ask. The finished reply ends with **Library documents read
for this answer**, links Cuckoo makes from its receipts, not the model; Chat opens them
in the same tab at the cited revision. Cited revisions are kept with page history.
Agents given anything in the Library get one instruction line about these tools.

Limits: an answer delivered to a conversation stays there if someone joins later; the
first release lists up to 100 documents per folder and offers up to 50 folders when
moving; uploads land unfiled and are moved afterwards.

## Agents and source continuity

Share grants agent reading separately from people access. The current picker uses
available managed workspace agents; the legacy private-runtime adapter is not
enabled for Library sources. Only me can still include a selected workspace agent
without sharing the file with other members.

Ask explicitly creates a new private Chat containing only the requester and that
agent. A durable job stores source item/revision/access-revision references, not
raw source text. The runtime retrieves bounded excerpts after fresh requester,
agent and item checks. Each source-bearing job's output audience remains limited
to the requester. Revocation, trash or an access-revision change fences queued/
running publication inside the same workspace transaction as the job claim check.
Content updates preserve the specifically selected historical revision.

The returned response lists source versions supplied to the request. Citations
identify supplied evidence, not a guarantee that every answer claim is correct.
Opening an older citation explains when the current page has since changed.
Source text is not copied into agent-wide knowledge; source-bearing temporary
runtime directories are removed after normal shutdown. The configured model
provider processes the explicitly requested question/excerpt. A crashed runtime's
temporary files require the same controlled retention/reconciliation as other
orphan data before wider deployment. Delivered private answers remain Chat records.

Reading adds no business tools, private runtime files, browser capability or
operation grants. Automatic document edits are not enabled; agents answer from Library
documents in shared conversations only through the tools above and their audience rules. Local simulation replies are scripted; they demonstrate
the request/permission/citation path, not real model understanding.

## Upgrade and verification

Apply forward migration 9 using the existing administrative migration path before
running the new PostgreSQL identity binary. Existing personal/business records are
preserved. Runtime database permissions must cover the new tables. On the local
development database `./tools/local-postgres.sh migrate` performs the migration
and runtime grants. This implementation's live review uses a dedicated simulation
database and storage root; ordinary identity servers/databases are not migrated
or restarted by that review.

Relevant checks:

```sh
cargo test --locked --lib
./tools/local-postgres.sh test
npm test --prefix web
npm run build --prefix web
./scripts/check-file-lines.sh
```

The ignored S3 adapter integration test requires a dedicated loopback emulator
and fixture bucket; see `src/identity/library/storage.rs`. Do not point fixtures
at production storage. Executed browser reviews cover admin/User page sharing,
revocation, private agent handoff, upload/download/ranges, concurrent edits, lost
acknowledgements, interrupted transfer, history restore, PDF preview and narrow/
short layouts. Model fixtures and storage emulation are separate from live cloud
and model-provider acceptance.

## Recorded verification — 2026-10-22

Passed: 65 Rust library tests, 36 real PostgreSQL tests, 72 frontend tests,
production client/debug-server builds, browser-test types, Clippy and source line
limits. The isolated S3 adapter test also passed against a loopback Moto fixture.
The source-bearing queue test retains its selected revision after 55 later edits
and proves revocation prevents publication. HTTP tests cover exact file completion,
checksum mismatch, protected originals, range/HEAD reads and mutation-origin denial.

Executed browser checks cover the complete journey with both local storage and the
S3 adapter through an emulator. Recovery checks exercise lost save acknowledgements,
concurrent edits, interrupted uploads across feature navigation, version restore,
PDF processing/preview and keyboard focus. A failed deployment-chunk check preserves
navigation and recovers through the explicit reload action. No real inference,
email, business operation or hosted-bucket request was made.

Reproduce browser journeys only against an explicitly configured isolated scripted
simulation with file storage. The drivers reject ordinary identity servers:

```sh
CUCKOO_LIBRARY_TEST_OUTPUT=.tmp/library-check \
  node tools/check-library-browser.mjs http://127.0.0.1:8890
CUCKOO_LIBRARY_TEST_OUTPUT=.tmp/library-recovery-check \
  node tools/check-library-recovery.mjs http://127.0.0.1:8890 .tmp/library-check/manifest.json
```

A consistent fixture database dump restored to a separate database with matching
rows across all eight Library tables. All three referenced original files matched
their saved size/SHA-256 after copying immutable objects. Evidence is local under
`.tmp/library-backup-review/`; it is not a production/cloud recovery guarantee.

The main startup JavaScript grew by about 2.5 KiB gzip versus the pre-feature build.
The editor feature is about 129 KiB gzip on demand; PDF preview is about 125 KiB
plus a 365 KiB worker gzip on demand. Hashed worker modules use the same private
immutable cache policy as other hashed static code. Protected documents remain
no-store. These are build-size measurements, not startup/latency guarantees.

## Recorded verification — 2026-11-05 (LB1)

- PostgreSQL (`postgres/library_folder_tests.rs`): items follow their folder's sharing and
  its access revision; items in folders can't be shared apart; only folder editors add
  to a folder; move in and out clears sharing; a non-empty folder can't go to Trash and a
  restored item comes back unfiled and private; moves are audited. A colleague without
  access gets the all-staff policy from `search_library`/`read_document` and a cited
  answer, while the HR folder is invisible to them; the owner may use HR privately but
  not in a conversation with the colleague; changing HR's sharing after a read withholds
  the reply. Editing levels, customer and unknown audiences are refused; Ask follows the
  folder's agents. The existing Library, source and upgrade tests pass.
- Real Goose (`tests/library_folders.mjs`): an admin creates a folder in the browser,
  shares it with an agent for all staff and sees it in the folder header; a colleague's
  question is answered with a "Library documents read" citation that opens the page;
  an HR document is never read for them or where they will read the answer. Screenshots
  at desktop, 390 px (no horizontal overflow) and dark.
- Web (`tests/library-folders.test.mjs`, `library.test.mjs`, `markdown.test.mjs`): folder
  sharing with Can edit and the safest audience by default; one folder per click burst
  with the same identity on retry and the name kept after a failure; Move offers only
  editable folders and sends the sharing revision; an open folder lists its own items;
  Library citation links open in the same tab and other relative links stay text.

## Dependency decisions

- Tiptap 3.32.0 and selected extensions (MIT): reused for structured editing,
  checklists, undo and React ownership. No paid collaboration/AI backend is added.
- object_store 0.14.2, AWS feature only (MIT/Apache-2.0): reused for server-side
  S3 credentials, conditional writes and object reads. No custom signing engine.
- PDF.js / pdfjs-dist 6.4.299 (Apache-2.0): reused for canvas-based PDF previews;
  no PDF scripting, forms or arbitrary HTML rendering is exposed by this viewer.
- Uppy is deferred with direct/multipart uploads. The small bounded queue uses
  native XMLHttpRequest progress and existing typed identity requests; it does
  not implement a multipart engine.

The editor and PDF viewer load on demand. Their compatibility was checked in
real browser journeys, with exact versions retained in the lockfiles. The existing
Vite 8.3.2 audit warning predates these additions; this change does not upgrade
unrelated development tooling.

Grid/collections, full-text indexing at larger scale, public links, realtime
co-editing and Notion-style databases remain outside this initial release.
Production rollout also needs the chosen storage provider, disclosed retention/
reconciliation policy, operational limits and live model/data-policy acceptance.

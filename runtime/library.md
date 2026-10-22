# Workspace Library

First implementation, 2026-10-22, on `feat/workspace-library`. The editable
[OpenPencil concept](../designs/library/README.md) remains the broader design
reference. This increment delivers the deliberately smaller first release.

## Available behavior

Library is a React feature in the existing workspace shell. PostgreSQL workspaces,
including solo workspaces, support native pages, private/selected/workspace sharing,
file uploads, protected downloads, stars, search, trash/restore and page history.
The legacy SQLite/local-owner deployment shows an explicit unavailable state.

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

Owner/User remain workspace membership roles. Both can create Library items.
Only Owners invite or remove workspace members through Settings.

- New items are private. The creator manages sharing and trash.
- Native pages offer Can view or Can edit. Edit does not grant resharing. The
  initial file-sharing UI offers view/download access; file replacement is deferred.
- Workspace visibility gives current members view access; named editors can edit.
- Another Owner does not automatically read private/selected-people content.
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
old grants. An Owner can explicitly take over a workspace-visible orphaned item;
an existing editor can take over a selected-people orphaned item. This does not
grant an Owner new read access to selected/private content. Private orphaned items
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
operation grants. Automatic document edits and source-backed shared-channel
questions are not enabled. Local simulation replies are scripted; they demonstrate
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
npm run check:e2e --prefix web
./scripts/check-file-lines.sh
```

The ignored S3 adapter integration test requires a dedicated loopback emulator
and fixture bucket; see `src/identity/library/storage.rs`. Do not point fixtures
at production storage. Executed browser reviews cover Owner/User page sharing,
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

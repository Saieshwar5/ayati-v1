# Cuckoo Library — implementation plan

Status: proposed implementation sequence, 2026-10-22. The nine-screen
[OpenPencil concept](README.md) exists; Library application code, cloud storage,
processing and agent-source integration are not implemented by this plan.

## 1. Product outcome and release boundary

Give every workspace one understandable place to write pages, keep files and
choose which people and agents can use them. Keep Owner/User workspace membership
unchanged. A solo workspace uses the same feature without needing team setup.

The first complete milestone is a real journey: create a private feature plan,
upload a PDF reference, share both with one existing User and one eligible agent,
ask privately in Chat, see a source citation, then revoke access and prove the next
read is denied. Failed saves and interrupted uploads must recover in this journey.

Deliver smaller usable increments on the way to that milestone. Do not expose
buttons for unimplemented agents, previews or recovery as if they already work.

Included in the first release:

- Native pages: headings, paragraphs, formatting, lists, checklists and references.
- Files: upload, original download, safe image/PDF preview and supported extraction.
- Private, selected-people and workspace sharing; separate selected-agent reading.
- All items, Recent, Private, Shared and Starred; search and paginated results.
- Flat list first, then grid, simple collections, revision history and trash.
- Private agent questions with citations; no silent agent edits or external actions.
- Light/dark themes, keyboard access, narrow windows and honest recovery states.

Defer simultaneous cursor-based collaboration, Notion-style databases, nested
permission inheritance, public links, guests, external-drive synchronization,
OCR, audio/video transcription, vector search and autonomous document rewriting.
Keep unsupported files storable/downloadable within configured limits, clearly
labelled when preview or agent reading is unavailable.

## 2. Verified foundation and integration boundaries

Inspected in the current checkout on `design/library-concept`:

| Existing foundation | Reuse and limit |
| --- | --- |
| Rust/Axum identity server and PostgreSQL workspace transactions | Reuse sessions, membership checks, mutation-origin checks, audit and transaction patterns |
| Owner/User Settings and invitations | Keep invitations and workspace administration Owner-only; item sharing selects existing members |
| React/TypeScript/Vite workspace shell | Add Library through existing same-workspace navigation and teardown boundaries; account/workspace changes retain native navigation |
| Guarded React request hooks | Reuse duplicate-submit protection and stale-read handling; scope every draft/cache by identity |
| Chat attachments | Existing local, chat-scoped uploads are limited to 10 MiB; they are not a general Library storage or permission model |
| App/panel resource grants | Existing mutations are Owner-only; do not repurpose them to prevent Users sharing their own pages |
| Approved agent knowledge and durable Chat jobs | Reuse access/claim patterns; Library retrieval must retain item permissions rather than copying private content into agent-wide notes |
| Eight PostgreSQL migrations | Add forward migrations after checking the branch at implementation time; preserve existing records |

Relevant starting points:
[Chat file handling](../../src/identity/collaboration/files.rs),
[object access](../../src/identity/postgres/objects.rs),
[resource grants](../../src/identity/postgres/resources.rs),
[agent knowledge](../../src/identity/postgres/agent_knowledge.rs),
[job claims](../../src/identity/postgres/jobs.rs),
[React requests](../../web/src/react/requests.ts),
[workspace application](../../web/src/react/workspace-application.tsx),
[navigation](../../web/src/navigation.ts),
[React boundary](../../runtime/react-client.md) and
[visual authority](../../runtime/design-system.md).

Start Library on the PostgreSQL identity deployment, including solo workspaces.
Preserve the legacy SQLite/private-runtime paths; do not build two complete Library
backends or treat an Owner's private runtime as the shared workspace filesystem.
Show an explicit unavailable/setup state on unsupported deployments.

## 3. Access contract before screens

All decisions below are proposed product contracts to encode and test in stage 1.
Workspace membership is always required, even when an old item grant still exists.

| Actor or action | Initial rule |
| --- | --- |
| Owner or User | Can create pages/files; new items are private |
| Creator | Can read, edit, share and trash their item |
| Selected person: Can view | Can open/download the current item; cannot edit or reshare |
| Selected person: Can edit | Can change content/metadata and replace a file with a new revision; cannot reshare or trash by default |
| Workspace audience | Current members can view; named editors receive explicit edit permission |
| Another Owner | No automatic access to someone else's private or selected-people content |
| Agent | Needs a separate item read grant and must be usable by the requester |
| Invite or remove workspace member | Existing Owner-only Settings controls |

People visibility is Only me, Selected people or Workspace. Only me may still
include selected agents. Changing visibility changes grants atomically using an
expected access revision; a stale sharing drawer must not undo a newer revocation.
Only the creator, or an explicitly recorded replacement steward, manages sharing.
Stewardship is an item responsibility, not a third workspace membership role.

Enforce these rules on list, detail, search, counts, versions, preview, thumbnail,
download, extraction retrieval and Chat requests. Filter before pagination and
snippet generation. Cross-workspace IDs must fail without disclosing item metadata.
Use current access for historical versions too; initially restrict history/restore
to creators and editors. Restoring creates a new revision.

Links and collections do not grant access. Start collections as personal,
single-level organization of accessible items; do not expose private collection
names to other people. Page references point to independently authorized items.
Show a restricted reference when necessary, without leaking its title. Offer an
explicit review of reference-file sharing rather than silently widening access.

Proposed departure policy: workspace-visible items remain with the workspace and
an Owner can assume stewardship. Selected-people items retain their audience;
replacement stewardship can go to an existing editor without adding Owner read
access. If none exists, retain them read-only pending an explicit recovery policy.
Private items become inaccessible on removal and follow a disclosed retention
policy; do not silently give their content to another Owner or reactivate old
grants when the person is re-invited. Resolve retention/recovery before the pilot.

## 4. Architecture and records

Use the existing React client, Rust service and PostgreSQL database with a private
S3-compatible bucket. Start with one operator-configured storage connection.
Ordinary users see files and available space; they never enter bucket credentials.
Any future Owner storage controls belong in Workspace Settings.

| Responsibility | Stored information |
| --- | --- |
| Item | Workspace, stable ID, page/file kind, title, creator/steward, visibility, current revision, access revision, timestamps, trash state |
| Page revision | Validated structured document JSON, schema version, author, plain text for search, previous/current revision relationship |
| File revision | Verified immutable object identity/version, original name, detected type, size, checksum, uploader |
| Grants | Workspace-bound human view/edit and agent read grants, separate from workspace membership |
| Personal organization | Stars, recent activity and collection membership scoped to the user/workspace |
| Upload session | Owner, target item/revision, reserved bytes, exact request identity, authorized storage operations, expiry, completion/cancellation state |
| Processing work | Source revision, job claim/lease, attempts, processor version, resulting artifacts and bounded error state |
| Audit and retention | Sharing/stewardship changes, revisions, trash/restore, processing and cleanup outcomes without file contents or secrets |

These are logical responsibilities, not a requirement for one table each. Keep
schema and modules focused. Use workspace-qualified keys/foreign keys so an item
cannot reference another workspace's grant, revision, collection or upload session.

Canonical native page content stays in PostgreSQL. Original binaries and derived
previews/extraction artifacts live in S3; the database stores their verified
identities. An object-key prefix organizes storage but never establishes access.
Do not add a vector database or another document service for the first release.

Proposed code boundaries: `src/identity/library/` for typed HTTP handlers and
validation, focused `src/identity/postgres/library/` storage/access modules,
one focused S3 adapter and processing worker, and `web/src/react/library/` for the
feature. Match the surrounding layout when implementing; keep source files below
500 lines. Extend typed Chat source requests rather than passing raw object URLs.

API families should cover authorized item listing/detail/create, content save with
expected revision, share updates with expected access revision, upload-session
create/status/sign/complete/cancel, preview/download, versions/trash/restore and
agent-source handoff. Reuse the existing identity route/session/mutation contract.
Return an explicit conflict for stale writes and stable status for an uncertain
completion. Bind idempotency keys to the exact actor, workspace and payload.

## 5. Ordered delivery stages

Every stage includes its own behavior tests and usable UI where applicable.
All stages below are planned, not completed implementation.

### Stage 0 — prove the small dependencies and storage contract

Evaluate Tiptap first for the small editor feature set and Cuckoo styling; compare
BlockNote only if block interaction materially reduces work. Exercise structured
save/load, pasted-content validation, keyboard use, undo, checklists, read-only
mode, React Strict Mode and theme styling. Record exact versions, license scope,
bundle change and exclusions; do not assume paid extensions are part of the core.

Evaluate Uppy with only needed React/S3 plugins and server-side operation signing.
Use the current API for the pinned version. Cuckoo chooses keys and validates the
session, method, part number and upload ID; never sign arbitrary client-supplied
bucket paths. Test the Rust S3 client against the intended provider before choosing
it. Keep credentials server-side rather than sending storage credentials to users.

The provider proof must cover CORS, small/multipart upload, checksums, cancellation,
expiry/renewal, safe finalization, immutable reads and range downloads. A local
S3-compatible fixture is useful for repeatable tests but does not establish actual
provider compatibility. Select provider/region and use a dedicated test bucket
before this proof; this plan itself provisions nothing.

Exit: recorded dependency choices and a reproducible real-storage round trip.
Stage 1 can proceed after the editor/access proof while the provider proof runs;
stage 2 requires the storage proof. Fix the access/departure contract and limits
before exposing the corresponding behavior.

### Stage 1 — private pages, sharing and a usable Library list

Add forward migrations and server-side item authorization, then wire Library into
the existing rail. Ship the empty state, paginated list, New page, editor, rename,
share drawer and opening a shared page as another existing member. Both Owners and
Users can create. Use the design's shared surface and focused editor layout.

Store validated document JSON with bounded depth/size, supported node types and
safe links. Reject raw executable embeds and unapproved remote content loading.
Use coalesced autosaves with one ordered in-flight save per page, revision checks
and payload-specific request IDs. Do not overwrite a newer server document.
Show Saving, Saved, Could not save and Conflict from actual request outcomes.

Keep failed drafts scoped to account/workspace/item. Recover a draft only after
fresh authorization; purge affected local drafts on sign-out or known revocation.
Offline recovery must not expose old content to a different account. Browser-local
copies cannot be remotely recalled while the device remains offline.
Define bounded history checkpoints instead of a permanent full snapshot on every
keystroke. Flush or warn on navigation; preserve unresolved conflicts for review.

Exit: two sessions create/share/edit a page; a concurrent edit never silently
overwrites, a removed grant denies a fresh read/save, and failed saves retain text.

### Stage 2 — S3 originals, upload recovery and authorized downloads

Add upload sessions, quota reservation, file revisions and the upload tray. Reserve
space atomically before signing; include concurrent uploads, retained versions and
trash in the documented quota policy. Failed/cancelled sessions release reservations
only after reconciliation, avoiding both oversubscription and premature reuse.

Transfer directly to private staging keys using short-lived signed operations.
Use multipart where it helps. The server verifies actual size/checksum and current
permissions before finalization; do not treat a multipart ETag as a SHA-256 digest.
Completion retries return the same outcome for the same request, including a lost
response after the object or database write succeeded.

A reusable signed PUT must not alter a file already accepted as a revision. Bind
reads to a verified provider object version, or finalize to a server-only immutable
key; use conditional create where supported. Test overwrite/replay races against
the selected provider. Finalization and reconciliation must handle the lack of one
transaction across PostgreSQL and S3 without destroying uncertain live data.

Keep the upload queue in a workspace-scoped controller so switching between
Library and Chat does not remount it. Dispose signing state/listeners and hide old
state on account/workspace changes. After reload, recover server session status
and request file reselection if bytes are unavailable; validate it before resuming.
Do not promise uploads continue after browser closure. Cancel/expire abandoned
multipart uploads and clean staging orphans through bounded reconciliation.

Use an authenticated streaming gateway for initial preview/original downloads,
including HEAD/range requests, so each new request checks current Library access.
Avoid a redirect to a reusable download capability when strict fresh checks are
required. Bound memory and stream large files. Share copies a Cuckoo application
link. Previously downloaded copies and an already active transfer are not recalled.
Use private/no-store response policies for protected content and clear in-memory
preview URLs/caches when access or identity changes.

Exit: upload, interrupt, resume/reselect, cancel, duplicate completion and removal
mid-upload work correctly; identical filenames create distinct items; revoked
members cannot request new bytes. File replacement deliberately creates a revision.

### Stage 3 — safe previews, extraction and useful browsing

Add bounded, restartable processing jobs keyed by file revision and processor
version. Reuse lease/claim patterns without coupling extraction to the Chat queue.
Only the active claim can publish artifacts. Handle trash, missing source objects,
duplicate work and stale revisions before publishing readiness.

Start with safe image thumbnails and text-bearing PDFs/plain text. Isolate parsers
with time, memory, page/pixel and output-size limits. Do not execute uploaded
content or render arbitrary HTML/SVG in the application origin. Use a reviewed,
restricted PDF viewer; unexpected types remain downloads. No OCR/transcription or
automatic third-party model submission. Keep temporary processing files bounded.

Display Uploading, Stored, Preparing for agents, Ready, Unsupported and Failed as
distinct facts. Preview availability and extraction readiness are independent.
All derived artifacts retain source access checks. Restrict operational logs to
IDs/state/errors; do not log extracted text or signed URLs.

Add permission-filtered filename/native-page search, then supported extracted-text
search using PostgreSQL. Add stars, recent items, personal collections, thumbnails,
grid and version/trash UI. Index updates are revision-aware; current access still
filters results when the index is stale. Avoid leaking hidden names through counts.

Exit: corrupt, oversized, encrypted and unsupported files give useful states;
worker restart does not duplicate artifacts; search, previews and history honor
current permissions. Original downloads remain available when extraction fails.

### Stage 4 — explicit agent questions with source citations

Implement Ask an agent using saved source item/revision references and an eligible
agent picker. Unsaved pages must be saved before handoff; do not silently discard
draft changes. Sending is an explicit user action. Initially allow private Chat
only; shared-channel source requests need a separate audience proof before release.

Use a private conversation between the requester and the selected workspace agent.
Extend the existing typed Chat job payload with source references. The current
owner-private runtime is a separate adapter: enable it only after proving the same
source boundary, otherwise show an explicit unsupported reason in the picker.

At submission, retrieval and result publication, validate current membership,
requester item access, agent usability, item agent grant, source revision and output
audience. A shared agent cannot become a proxy for other members' private files.
Use bounded excerpts with item/revision citations, freshness information and a
clear unsupported/not-ready state. Recheck after queued work or access changes.
Cancel/withhold results when source access changes during generation.
Fence publication with the active job claim and current source-access revision in
the publishing transaction, so a concurrent revocation cannot pass a stale check.

Treat document content as untrusted reference material. Library read access does
not grant tools, browser access, business operations, permanent memory or broader
knowledge sharing. Do not retain raw sources in reusable agent context/logs. Model
processing follows the configured provider/data policy; it is not implied by local
extraction. Private replies already delivered remain conversation records under a
disclosed retention policy; revocation prevents fresh retrieval, not recollection.

Start with answers and suggested text in Chat. Direct agent editing, automatic
patch application and source-backed shared-channel replies are later increments.
A future accepted suggestion must still check edit access and expected revision.

Exit: the complete milestone in section 1 works, citations identify the actual
version used, and another requester cannot obtain the same private source through
the same agent. A queued request revoked before publication produces no source reply.

### Stage 5 — reliability rehearsal and limited rollout

Exercise the scenario matrix below against real PostgreSQL and repeatable storage/
model fixtures. Run focused live browser journeys and a separate actual-provider
storage test. A fixture model does not establish live model/provider behavior.
Keep Library behind a deployment feature flag until permission and recovery gates
pass. Do not mix the pilot database/bucket with ordinary user data.

Set/document per-file size, workspace storage, active uploads, processor parallelism,
source-excerpt limits and retry budgets. Measure list/search/save latency, queue age,
upload failure/recovery, processing failures, storage growth and gateway bandwidth
on representative fixtures before assigning performance or cost commitments.

Test coordinated PostgreSQL/S3 backup and restore, including revisions and grants.
Referenced immutable objects and retained trash must survive restore. Storage
lifecycle rules must not expire live versions. Start garbage collection with a
report and quarantine; enable irreversible deletion only after proving retention
and orphan reconciliation. Rollback disables the feature without dropping data.

Exit: a solo workspace and a small multi-person workspace complete the full journey
after a service restart and restore, with no cross-workspace disclosure or lost
acknowledged document/file revisions in the tested scenarios.

## 6. Scenario matrix and validation

| Area | Required scenarios |
| --- | --- |
| Membership | Solo Owner; multiple Owners/Users; role change; removal while editing/uploading; re-invitation does not revive revoked grants |
| Isolation | Cross-workspace item/revision/upload IDs; old signed operation; hidden titles in list/search/counts/references; workspace/account switch during requests |
| Sharing | User shares own item; editor cannot reshare; workspace view plus named edit; simultaneous share/revoke; private content hidden from other Owners |
| Documents | Failed save; duplicate submit; uncertain success; two editors; navigation/reload; large paste; invalid schema/link; restore creates a new revision |
| References | Page accessible but attached file denied; explicit joint sharing; source trashed or revision replaced; no automatic grant from link posting |
| Uploads | Small/multipart; dropped network; expired signature; wrong reselected file; checksum mismatch; cancel/complete race; reused PUT after acceptance |
| Storage recovery | Object exists but DB result lost; DB points at missing object; duplicate worker; quota races; incomplete multipart; restart and backup/restore |
| Downloads | Denied original/thumbnail/preview/history; range/HEAD access; streaming limits; denied retry after revocation; no public object access |
| Processing | Corrupt/encrypted PDF; huge page/pixel count; unsupported type; timeout; stale revision; deleted source; failure does not remove original |
| Agents | Requester or agent denied; private source through shared agent; revoked queued/in-flight job; unready source; changed revision; source instructions do not grant actions |
| Retention | Creator departure; private versus shared stewardship; trash/restore; non-reactivation of old grants; GC never removes referenced versions |
| UX | Keyboard/focus restoration; light/dark; 320px and short windows; long titles; error/empty/loading; upload continues across same-workspace feature navigation |

Run the relevant Rust/PostgreSQL and React behavior tests per increment. At complete
feature gates use `cargo test --locked --lib`, `./tools/local-postgres.sh test`,
`npm test --prefix web`, `npm run check --prefix web`, `npm run build --prefix web`,
`npm run check:e2e --prefix web`, focused executed browser journeys and
`./scripts/check-file-lines.sh` when available. Browser-test type checking alone is
not a live browser test. Check dependency/bundle changes when adding editor/upload
packages. Keep fixture results separate from cloud integration evidence.

## 7. Branches, review units and decisions before pilot

Build from the Owner/User Settings work once merged, or explicitly stack the first
Library branch on that work. Do not publish a large mixed PR accidentally including
unmerged Settings changes. Keep the editable design and this plan as references.

Suggested review units: dependency/storage proofs; private pages with sharing;
S3 uploads/download/recovery; previews/search/organization/history;
private agent source handoff; recovery rehearsal and rollout controls. Each unit
contains its meaningful tests and current runtime documentation. Split further
only where an increment remains complete and reviewable.

Resolve before a public pilot: storage provider/region and supported signing
features; configured quotas/file types; retention and creator-departure recovery;
private-draft persistence; treatment of delivered source-based Chat replies; model
provider/data policy; and measured operational cost. Use the proposed simple
defaults above for development fixtures, not undisclosed production promises.

No application code, dependencies, migrations, cloud resources or production
configuration were changed to create this planning document. Documentation link
and diff checks validate the plan artifact; implementation acceptance remains open.

## 8. Primary references for implementation experiments

Rechecked 2026-10-22. Exact versions and integration behavior must be recorded
during stage 0 rather than inferred from documentation.

- [Tiptap editor and extension model](https://tiptap.dev/docs/editor/getting-started/overview)
- [Uppy S3 signing, multipart and compatibility](https://uppy.io/docs/aws-s3/)
- [AWS signed URL reuse, overwrite and expiration behavior](https://docs.aws.amazon.com/AmazonS3/latest/userguide/using-presigned-url.html)
- [AWS conditional writes](https://docs.aws.amazon.com/AmazonS3/latest/userguide/conditional-writes.html)
- [Earlier research and licensing boundaries](README.md#open-source-references-reviewed)

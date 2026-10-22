# Cuckoo Library — design proposal

Created in the live OpenPencil desktop through its authenticated MCP bridge on
2026-10-22. These are editable design frames and illustrative previews, not a shipped
Library feature or a working S3 integration. The existing application was not edited.

The [implementation plan](implementation-plan.md) turns this concept into ordered
delivery stages, access rules, storage boundaries and testable release gates.
It is a plan, not an implementation-status claim.

Open [cuckoo-library-concept.fig](cuckoo-library-concept.fig) in OpenPencil. It contains
nine named screens, arranged as a reading flow. Names, files and counts are fictional.

| Screen | Preview | Purpose |
| --- | --- | --- |
| 01 — All items | [Overview](library-overview.png) | One place for pages and files, with visible people/agent access |
| 02 — Write a page | [Editor](library-editor.png) | Focused document, save state, references and agent access |
| 03 — People and agents | [Sharing](library-sharing.png) | Separate people sharing, edit rights and selected agent access |
| 04 — File preview | [Preview](library-preview.png) | Read a PDF without leaving Library; inspect metadata and readiness |
| 05 — Upload and recovery | [Uploads](library-uploads.png) | Background tray distinguishing stored bytes, preparation and interrupted upload |
| 06 — Mobile | [Mobile](library-mobile.png) | Narrow layout retaining the existing global rail |
| 07 — First use | [Empty](library-empty.png) | Start a page or upload; no pre-created department structure is required |
| 08 — Ask an agent | [Agent handoff](library-ask-agent.png) | Select an eligible agent/source and continue privately in Chat |
| 09 — Visual browsing | [Grid](library-grid.png) | Thumbnail browsing for images, PDFs, pages and tabular files |

## Navigation and browsing

Add one Library entry for all workspace members. Preserve Cuckoo's quiet grey rail
outside the rounded shared workspace surface. Library is a daily work feature;
workspace-wide storage policies belong in Settings.

Use a flat list by default. Its columns answer four questions: what is this, who
can open it, which agents may use it, and when did it change? Type icons distinguish
native pages, images, PDFs and other files. Offer a grid when visual recognition is
more useful than scanning titles. Do not create a separate silo for every file type.

Local views are All items, Recent, Private, Shared and Starred. Collections are
optional organizational groups, not new workspaces or membership roles. Start with
no required collections. Show only records the current person can access, including
in search, counts, suggestions, previews and thumbnail requests. Specify collection
sharing semantics before implementation; moving an item must not silently broaden
its audience. This concept uses explicit item access as the initial authority.

New page and Upload are the two primary creation actions. New items start private.
Row actions can provide rename, move, star, share and trash when permitted. Paste a
stable Cuckoo link into Chat without automatically changing the item's access.

## Documents and files

A native page supports headings, paragraphs, lists, checkboxes, links and attached
references. Keep a readable content width, save state and a version-history entry.
Retain failed drafts and reject stale overwrites. “All changes saved” must mean the
server acknowledged the current revision. Recoverable drafts and trash need defined
retention rules before a pilot.

The focused editor hides the local Library navigation while retaining the global
rail and a back/breadcrumb path. The side column contains a short page outline and
agent access. It is secondary information, not another mandatory form.

File preview keeps the original visible and places metadata/access alongside it.
Preview is a derived representation; downloading returns the original file. A PDF
being stored does not establish successful text extraction. Show Preparing for
agents, Ready for agents or an understandable unsupported/error state based on
actual processing results. The illustrated PDF is a fictional, already-processed
example. Readiness does not grant any agent access.

Draft/Reviewed labels and review freshness are proposed refinements. They should
not become a mandatory approval workflow for every personal note.

## People and agent access

Owner and User remain the only workspace membership roles. Can view and Can edit
are permissions on an item. The creator initially manages sharing; editing alone
does not grant resharing authority. Sharing selects existing workspace members;
it does not let a User invite new people into the workspace.

The proposed people choices are Only me, Selected people and Workspace. Agent
selection is independent. “Only me” can therefore still include selected private
agent use. Owners administer the workspace and shared content; this proposal does
not silently grant other Owners access to private drafts. Private-content retention
and shared-content stewardship when a creator leaves need an explicit policy.

Start agents with read access and suggested changes for review. For an interactive
request, validate the requester, agent, workspace, source revision and intended
output audience. A document grant to a shared agent cannot let another requester
or a channel audience obtain private material. The Ask action starts a private Chat
handoff; it does not automatically post to an existing shared channel or execute on
opening the drawer. Recheck permission when sending and when retrieving sources.

Reference Library documents by identity/revision. Do not copy private documents
into broadly available agent-wide notes as an access shortcut. Retrieval, extracted
text, summaries and cached context must retain the original access boundary.
Reading a document never grants permission to perform a business operation.

## S3-backed storage proposal

People interact with documents and files. Buckets, keys, regions and credentials
stay out of ordinary Library flows. Workspace Owners may eventually configure a
storage connection in Settings; it is not repeated on each upload.

Recommended responsibilities:

| Cuckoo database | Private S3-compatible storage |
| --- | --- |
| Workspace/item identity, title, creator and collection | Original uploaded bytes |
| People and agent permissions | Generated thumbnails and previews |
| Current native page content/revision and revision metadata | Immutable file-version objects and optional page snapshots |
| Upload sessions, readiness, trash and audit | Extracted artifacts when appropriate, under the same access rules |

The database remains the authority for access. Object-key prefixes help organize
workspaces but are not authorization. Generate opaque, workspace-scoped keys on the
server, keep storage credentials server-side, and do not use filenames as writable
object identities. Two uploads called `plan.pdf` must not silently overwrite each
other. Replacing an existing file should create a new logical revision deliberately.
S3 object versioning alone is not the Library's document-history UX.

Proposed upload flow:

1. Choose files or drag them into Library. Show destination, private access and
   any size/quota limits before transfer.
2. Create an authorized upload session. Transfer directly to storage using narrowly
   scoped, short-lived signed operations; use multipart upload where appropriate.
3. Show per-file progress, cancellation and recoverable interruption. Keep upload
   session progress across navigation; after reload, request the source file again
   if the browser cannot recover its bytes. Do not promise automatic continuation
   after closing the browser without proving the chosen implementation supports it.
4. Verify the completed object, size/checksum and current membership before marking
   it stored. Handle lost completion responses without creating duplicate items.
5. Generate previews/extracted text asynchronously. Show preparation separately from
   transfer. Local extraction is not permission for an external model to read a file.
6. Share deliberately. Stored, searchable and agent-readable are distinct states.

Expired signing URLs should be renewed only after authorization; cancellation or
abandoned multipart sessions need cleanup. Define storage quotas, trash retention,
per-file limits, parallelism and storage-full behavior before implementation.

Share copies an application URL, not a raw signed object URL. AWS signed URLs are
bearer capabilities that can be reused until expiry. Removing a Library grant stops
new authorizations but does not automatically invalidate a previously issued URL.
Use short lifetimes; where strict access revocation on new downloads is required,
use an authenticated download/preview gateway. Already downloaded copies and active
streams cannot be recalled. S3-compatible providers must be checked for the selected
multipart/checksum/signing behavior rather than assuming full AWS equivalence.

## Open-source references reviewed

Research checked on 2026-10-22 from official repositories/documentation. These are
candidates and product references, not dependencies installed or integration-tested
in Cuckoo. The visual layouts here are original editable Cuckoo designs.

| Project | Useful ideas or reuse candidate | Current licensing/source boundary |
| --- | --- | --- |
| [Docmost](https://github.com/docmost/docmost) | Page organization, history, attachments and permission UX; its configuration supports S3 storage | Core AGPL-3.0; enterprise directories have a separate license. Use as a product reference; assess compatibility before copying code |
| [Tiptap](https://tiptap.dev/docs/editor/getting-started/overview) | First editor candidate: a headless editor that fits existing React controls and a small chosen feature set | Open-source editor is MIT; Pro/Cloud features are separate. Its core alone is not the full document backend |
| [BlockNote](https://github.com/TypeCellOS/BlockNote) | Alternative candidate for a ready-made React block editor, slash menu and block interaction | Majority MPL-2.0; XL packages GPL-3.0/commercial. Evaluate the actual selected packages |
| [Uppy](https://uppy.io/docs/aws-s3/) | Upload queue/progress, failure recovery and S3 multipart upload; React headless UI/hooks can retain Cuckoo styling | MIT project; use only needed plugins and Cuckoo-authorized signing endpoints |
| [Outline](https://github.com/outline/outline) | Additional reference for a calm, focused knowledge/document product | Source-available BSL 1.1, explicitly not an open-source license; current grant restricts Document Services. Do not assume its code can be embedded in this service |

Recommendation: study Docmost for product behavior, evaluate Tiptap for the editor
and Uppy for uploads, and keep Cuckoo's Rust identity/access/storage boundary. Compare
BlockNote if a ready-made block experience saves enough work. Avoid importing a
complete second product stack or implementing a rich editor/upload engine from scratch.
An editor and uploader do not supply document permissions, persistence or agent safety.

Primary references:

- [Docmost features and core/enterprise licensing](https://github.com/docmost/docmost/blob/main/README.md)
- [Docmost S3 storage configuration](https://github.com/docmost/docmost/blob/main/.env.example)
- [Tiptap editor and extension model](https://tiptap.dev/docs/editor/getting-started/overview)
- [BlockNote features and package licensing](https://github.com/TypeCellOS/BlockNote/blob/main/README.md)
- [Uppy S3 integration](https://uppy.io/docs/aws-s3/) and [React integration](https://uppy.io/docs/react/)
- [Outline license](https://github.com/outline/outline/blob/main/LICENSE)
- [AWS signed URL semantics](https://docs.aws.amazon.com/AmazonS3/latest/userguide/using-presigned-url.html)
- [AWS multipart upload/recovery](https://docs.aws.amazon.com/AmazonS3/latest/userguide/mpuoverview.html)

## Design verification and next milestone

All screens were rendered by OpenPencil and inspected as exported images. The
OpenPencil text contrast/minimum-size checks pass, with no reported frame/text
parent overflow. Liberation Sans faces are available without font substitution.
Fixed mockups do not establish keyboard behavior, responsive layout code, real
permissions, cloud uploads, extraction quality or agent execution.

The first implementation milestone should complete one actual journey: create a
private page, upload a reference, share with one existing User and one eligible
agent, ask privately with a source reference, and prove revocation on a subsequent
read. Test upload interruption and failed/stale document saves alongside it. Choose
the editor/uploader after a small real compatibility and bundle-cost experiment.

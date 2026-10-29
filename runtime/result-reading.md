# Compact responses and saved reading

Implemented locally as the next serial increment after
[questions PR 68](https://github.com/Saieshwar5/ayati-v1/pull/68).
The [editable concept](../designs/result-reading/result-reader.pen) and checked
[desktop](../designs/result-reading/reader-concept-desktop.png) /
[phone](../designs/result-reading/reader-concept-mobile.png) previews were saved
in connected OpenPencil before application changes.

## Saved reading contract

Long agent replies stay compact from their first streamed render. A deterministic
excerpt never invents a summary. Initial bounds are 900 characters/eight line
breaks for structural classification, a 320-character excerpt, and a bounded
156-pixel text region inside a softly filled box (up to 226 px with padding and
its corner control); measured overflow exposes expansion on narrow windows.
The selected [corner-arrow concept](../designs/agent-response-box/README.md)
replaces Chat's labelled Read response footer and character count. Short replies
use their natural height. Long replies fade at the bottom and expand their saved
body inline through a bare arrow icon with a 44 px keyboard/touch target. A reverse
arrow collapses; expanded replies also expose a top collapse icon. No inner
vertical scrollbar is added. On phones, the box spans below its author row.

Expanded replies expose Copy, exact Markdown download and a secondary full reader
as quiet icons. Questions, approvals, attachments and reply Stop remain separate.
Chat no longer shows Check progress launchers; existing task APIs and Agents Work
remain available. Human input and explicit approval/question controls retain their own
semantics. One saved message identity owns its streamed output.

Full content already persists in PostgreSQL messages/ordered output receipts and
existing private SQLite messages. Reuse those stores rather than duplicate every
streamed draft into a second archive. Saved bodies have a content-derived version;
completed messages retain that exact body across reload. Interrupted managed replies can reconstruct the latest retained attempt from
ordered output receipts, without changing the cancellation message or mixing
generations. This increment
does not promise an archive of every past intermediate draft. Opening reads the
latest authorized saved draft or complete response and freezes the displayed
body. A newer saved state is offered explicitly; reading position never jumps to
new content automatically.

New reads use `/api/identity/workspaces/{workspace}/chats/{chat}/messages/{id}/response`.
Reader bodies are
bounded to 512 KiB; larger stored responses remain intact and fail explicitly in
the reader. Existing managed execution/output limits remain 64 KiB. Markdown
downloads retrieve the exact saved complete body/version; there are no invented
PDF/Word/Excel exports. Copy uses the explicitly opened body.

Workspace history/focus/SSE may opt into `compact=true` without breaking existing
full-message clients. All read paths retain current conversation/Library access,
including completed private-source outputs and alternate history delivery. Opening,
copying, refreshing and downloading do not create a task or invoke a model.

Existing file/object IDs remain the source of filenames, bytes, media types and
versions. PNG/JPEG thumbnails are bounded; full image bytes load on explicit open
with zoom, fit, close and original download. Active formats and model-written
remote image URLs do not execute or make automatic tracking requests. Unsupported
previews retain an authorized original download. File groups remain bounded.
Downloads recheck current rights and preserve filename, MIME and exact identity.

One shared saved-response lifecycle serves inline replies and the full reader,
including source/version checks, aborts, explicit newer-version loading and
revocation. Workspace messages and threads use the same inline component. React
owns stable response islands and the external reader portal. Refreshes reuse those islands, preserving expansion,
frozen saved text and focus; removed messages/view teardown unmount their reads.
Expanding suspends transcript auto-scrolling until all reading views close. Close
returns to the same conversation, draft, scroll and current launcher. Source
revocation/identity changes clear pending reads and displayed private content.

## Verification

The 2026-10-26 corner-arrow refinement passes 183 web tests, production build,
browser-test types and all 882 source file limits. Connected local journeys cover
saved reading/files in workspace/thread and private runtime, source-derived helper
revocation clearing both inline/full views, and read-only progress with retained
Chat questions, restart and exact Stop. Desktop, 390 px dark and 320 px layouts
were inspected. Evidence and actual captures are linked from the selected design.
These use local pinned Goose with scripted inference, not paid/cloud acceptance.

Check short/long/streaming/history/fallback text, code/tables, narrow/short windows,
source/version conflicts, failed reads, stale responses and unmount cleanup. Run
real-Goose/scripted-model journeys with persisted full text after restart, no
inference on reading, actual image bytes and downloads, private managed and local
entry points, wrong-scope denial and Library/source revocation. Inspect the real
browser layouts and retain evidence. Generated work-product publication/export
and universal O3/O4 acceptance remain separate until actually proven.

## Image dependency and bounded work

The focused Rust `image` crate is pinned to **0.25.10** with only `png` and `jpeg`
features; default formats and Rayon are disabled. It is maintained by
[image-rs](https://github.com/image-rs/image) under MIT OR Apache-2.0, compatible
with Ayati's dependency use. It supplies decoding, camera orientation and bounded
thumbnail generation without another system image-conversion process. No existing
locked dependency was upgraded; sixteen packages were added to the lockfile, including this dependency.

Preview workers admit at most two concurrent jobs, acquiring a permit before
reading original bytes. Width/height are capped at 8192, total pixels at
16,777,216, decoded bytes at 64 MiB and derivative edges at 256 pixels. No upscaling
occurs. The decoder's 64 MiB allocation hint is best effort, as documented in
[image Limits](https://docs.rs/image/0.25.10/image/struct.Limits.html); it is not a
hard process-memory guarantee. Original input limits remain 10 MiB for workspace
objects and 25 MiB for private artifacts. A busy/unsupported/malformed preview
retains the original download path. Real PNG/JPEG checks verify dimensions and
EXIF orientation, and reject malformed/active/oversized inputs.

Object and private-file routes accept `metadata=true`, `access=true`,
`thumbnail=true`, `inline=true`, `preview=true` and an exact checksum `version`.
Default navigation downloads the original with an encoded stored filename.
Only passive PNG/JPEG/audio media can render inline. Supported text/source formats
render as plain text up to 64 KiB; SVG/HTML never become active documents. Managed
objects verify stored size/checksum and recheck source access after storage I/O.
Private archives use regular-file/no-symlink reads and verify a requested checksum.

Open readers recheck metadata every five seconds while visible and on workspace
access/reset notifications. File viewers use a cheap access check while open.
Denied reads clear the content; offline failures remain explicit. These checks
cannot retract bytes someone already legitimately viewed or downloaded.

## Verified locally

- 95 Rust unit, 147 standalone disposable PostgreSQL and 120 web tests pass.
  The additional PostgreSQL controller/MCP test passes through its required
  authenticated process fixture. Eight local API/storage integration tests pass.
- Ten unique process/browser journeys have passing final checks: the two new
  reader/file journeys, five retained streaming/recovery journeys, the existing
  private Chat panel journey and two shared controller/MCP journeys.
- The new real-Goose journey proves compact streaming, a frozen body and physical
  reading-area position, explicit update, exact copy/Markdown bytes, source/version
  denial, restart recovery, actual thumbnails/original downloads, image fit/zoom,
  inert SVG text and nested thread Escape. The private runtime checks bodies older
  than the latest 200-message page, restart, exact archived images and agent denial.
- Clippy with warnings denied, production builds, browser-test types and source
  file limits pass. [Design captures](../designs/result-reading/README.md) were
  visually inspected. Evidence/logs remain under `.tmp/result-reading/`.

Reproduce with `cargo test --locked --lib`, web check/build/test commands and,
`./tools/local-postgres.sh test`. Reader journeys run with
`node --test tests/result_reading.mjs`; retained journeys are
`tests/reply_stream.mjs` and `tests/agents_workspace.mjs`.

This reader increment introduced no migration and shipped on **schema 23**.
Current startup follows [PostgreSQL setup](postgres-workspaces.md). No normal workspace
database was migrated during its checks, and no cloud or
paid model was used. This delivers the saved text/file/image reader subset.
Generated artifact publication, PDF/Word/Excel export, every historical streamed
draft, rich PDF/media readers and universal O3/O4 acceptance remain separate.

# Durable personal workspace Home panels

Implemented locally after PR 77 (`04781747`), following PR 76. This contract and the selected
OpenPencil concepts were saved before application edits. This is the first managed-workspace P1 panel increment. Existing adopted
local-owner panels retain their separate runtime and source contracts.

## Supported product slice

A member explicitly asks a currently permitted workspace agent, in an existing
accessible conversation, to prepare a personal Home panel. The ordinary task and
current control gateway handle the work. `personal_panels` lists only the original
person's opaque panel identities/revisions for this agent and conversation; `save_home_panel` creates or
revises one typed, bounded draft. The controller fixes workspace, human owner,
maintaining agent and task provenance. Neither models nor agent maintainers can
select another owner, audience, layout, permission set or publication decision.

Reuse the established Metric/List/Chart/Agenda/Brief validation. Save immutable
PostgreSQL snapshots independently of workers. Human **Keep on Home** reviews an
exact revision; each later revision requires review. The previous approved view
remains until a new exact revision is accepted. No HTML/scripts, arbitrary remote
media, new publication authority or inferred source truth enters a panel.

A person's arrangement, standard/wide choice and hidden placements persist in
PostgreSQL per workspace/user. Hiding a placement changes no task or maintenance
permission. Opening/reloading Home reads saved records and starts no inference.
Panel creation/revision/activation/layout retries retain exact receipts and stale
writes fail. Limit the bounded initial personal collection to 32 panels and 100
revisions per panel; these are operational bounds, not subscription entitlements.

## Sources and authority

Viewing/reviewing requires the original person's current workspace/conversation
and private-agent visibility plus the existing complete Library/event/handoff
source guards. Automatically capture completed logical-task operation receipts
and their effective rule/account revisions; caller-supplied source IDs must be
from that task. Revoked/reconfigured source authority hides the corresponding
cached snapshot, including previous active views and future revisions retaining
that provenance. A panel revision carries prior panel provenance rather than
silently dropping sources. Receipt scope does not prove an agent interpretation.
No-receipt findings are explicitly labeled. Unconfirmed external outcomes remain
unconfirmed. Creation is fenced by the current exact worker capability, original
human, saved task/lease, pending input/action and handoff checkpoint rules.

Personal invalidation events use the current durable transport and a server-side
recipient filter, exposing no private panel IDs/titles to other people. Clear
cached content before a source recheck; ignore stale reads and clean listeners,
timers and portals on identity/view changes. Source authorization is checked on
all HTTP/tool reads and human mutations, independently of UI presentation.

## Implementation and acceptance order

1. Inspect current panels/templates, managed tools, Home compatibility subtree and
   source/privacy boundaries; save selected OpenPencil desktop/phone concepts.
2. Schema 30: private ownership, immutable snapshots, exact human/layout receipts,
   personal arrangement and audience-filtered invalidation. Reuse source and tool
   boundaries; migrate disposable fixtures only.
3. Add bounded agent draft/list tools and exact human review/layout APIs. Add a
   React workspace-panel slice in the documented Home compatibility host without
   replacing the adopted local-owner panel controller.
4. Prove exact/semantic retry, wrong owner/tenant, stale review/layout, source
   revocation/regrant, immutable snapshots, template and collection bounds,
   migration/restart and no inference on Home reads. Exercise real pinned Goose
   with scripted inference, human review, second-device placement, a later draft
   retaining the approved view and current-source denial in a live browser.
5. Inspect desktop/phone/dark captures, run focused plus risk-appropriate checks,
   update evidence and publish one dependent draft PR before another feature.

Shared audiences/management, recurring maintenance bindings, deterministic refresh,
archive/reassignment, artifact-backed panel payloads, real provider usefulness,
paid inference and cloud capacity/cost acceptance remain later increments. This
private P1 slice does not claim all P1/P2 or whole-product launch acceptance.

## Implemented boundaries and local evidence

Schema **30**, `m20261026_000030_workspace_home_panels`, adds private panel identity,
immutable version/provenance records, exact human/layout receipts and per-person
arrangement. New controllers require explicit migration; see
[PostgreSQL workspaces](postgres-workspaces.md). Only disposable fixture schemas
were migrated here. Offline AWS release checks require 30 and keep every activation
gate disabled; no AWS action or normal workspace migration was performed.

`personal_panels {}` returns only the current original person's panel identities
and revisions for this managed agent in the original conversation. No saved titles or facts
are returned through discovery. `save_home_panel` reuses `PanelDraft` and the
fixed-template validators. It rejects publication/update-policy flags and fields
that select another person/workspace. The exact current worker capability, lease,
original task, request permission, pending work and same-agent ownership are checked
again by the controller. Semantic repeats with a new model-generated panel ID in
the same task return the saved canonical draft receipt. A changed existing draft
needs its current expected revision. Saving grants no publication or tool rights.

The collection is bounded to 32 personal panels, 100 revisions per panel, 32 retained
task sources and 32 operation receipts per provenance chain. Expanding beyond that
source budget fails rather than dropping provenance. Caller-provided source IDs
must be completed receipts in the same bounded logical task tree; successful
observations are included even when the model omitted them. Revisions retain
previous provenance. Source/account/rule changes cannot relabel old observations
as current; regrant does not restore snapshots pinned to old source versions.
Receipts describe observations, not factual accuracy, freshness or successful
unconfirmed effects. No-receipt findings remain explicitly labeled.

Human API:

- `GET /api/identity/workspaces/{workspace}/home-panels`: bounded personal list,
  current authorized draft/approved snapshots and saved arrangement. Another Owner
  gets no rows or private titles merely because they manage the workspace.
- `POST .../home-panels/{panel}/review {request,revision}`: exact current draft,
  current source access and original person's own panel only. Exact old request
  acknowledgments do not reapply publication after later revisions.
- `POST .../home-panels/layout {request,revision,order,wide,hidden}`: compare-and-set
  personal placement. IDs must belong to the person; stale device writes fail.
  Hiding changes no source, task, automation, approval or shared authority.

A recipient-filtered `panel_access` live event contains no private IDs/titles and
clears cached Home snapshots/details before rechecking. Existing membership,
conversation and Library/event/handoff guards are retained. The React slice is a
lazy Home portal in the documented compatibility subtree; adopted local-owner
panels keep their old controller, source and browser-local placement contract.
Unadopted managed workspaces receive the new explicit request/review controls.
Managed desktop briefs use three grid rows so a short finding remains visible
below its title, purpose and heading. Metrics retain their smaller preset and
phone cards retain their bounded height. Saved content is a named, focusable
region with a visible keyboard focus indicator; End/PageDown can scroll long
content while review/details controls remain accessible. The full detail reader
restores focus to its opening control. See the [readability design](../designs/home-brief-readability/README.md)
and `tests/home_brief_reading.mjs` for the regression and local browser evidence.

Compact panel content scrolls within its preset while review/details controls stay
visible. The Sources and details reader shows the complete snapshot and basis
without publication or inference. Dialog/fetch/subscription lifetimes are owned.

Current checks: 102 root Rust unit and eight guest-bridge checks, 228 disposable
PostgreSQL checks (nine new Home cases), 169 web checks (six new Home cases),
52 offline Python checks, Clippy, Rust/web builds and browser-test types. The
populated-v29 migration preserves an existing waiting question and reconstructs
its same task after an answer. Template/collection bounds, exact/semantic retry,
wrong principal/tenant/worker, stale review/layout, immutable snapshots, private
Owner denial, cross-conversation discovery/revision denial, receipt/source retention and revocation/regrant are exercised.

Real pinned Goose with scripted local inference completes an explicit member's
Home request, performs one authenticated read, saves two personal drafts, obtains
human review and revises a view while preserving its previous approved snapshot.
Order/hide/restore sync between two authenticated browser sessions; controller
restart preserves views/layout without inference. Source revocation hides only the
affected saved view while an unrelated personal panel remains readable. The journey
records nine scripted model requests, one MCP read and zero external writes.

A second connected journey saves an event-derived helper result as a personal Home
panel. Revoking that helper's read clears its open Home detail reader together
with the parent response/progress and all derived source reads. This journey makes
five scripted model requests and zero MCP calls. These are synthetic local
sources; genuine provider usefulness, cloud capacity/cost, production recovery and
whole-product launch remain unproved. Broad regression results and captures are
indexed in `.tmp/workspace-home-panels/verification.json` before publication.

## Source-discovery refinement before delivery

Bind a personal panel to its original conversation as well as its human/agent.
Agent discovery returns opaque IDs/revisions only, never saved titles or facts,
and filters to that current conversation. Revisions reject another conversation.
This prevents a later shared-channel task from discovering a private-source panel
created in a different conversation. Human Home reads retain their source checks.
**Ask for changes** on a currently readable panel explicitly binds the request to
its original agent/conversation and includes its selected ID/revision. The form
requires current independent request permission and never sends on opening.

The final broad run passes **21 connected process/browser cases** on the completed
stack, including the isolated approval-review prerequisite. Earlier failures in
that broad check led to PR 77, which was separately verified on its clean base.
Final source privacy and human change-target checks pass on the updated schema 30
build. All 830 source files remain within 500 lines. The personal panel feature is
published after that prerequisite; reviewed ancestors reach main through PR 78.

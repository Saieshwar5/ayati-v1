# Managed-work attention

Implemented and locally verified on `feat/scoped-attention-inbox`, 26 October
2026, serially after [Chat organization](chat-organization.md), PR #70. The plan
below was saved before application edits; verification is recorded separately.

Home's Notifications panel includes managed PostgreSQL work. A bounded
inbox has **Needs you** and **Recent work** views, with source conversation/thread
links and the existing exact question/approval reviews. The native private
runtime's current inbox remains reachable through an explicit separate adapter.
Home panels, its clock and saved work records retain their current behavior.

## Audience and authority

- Pending questions go to the original requester. Pending exact approvals and
  unconfirmed action outcomes go to the saved designated approver.
- Finished work goes to its original requester. Failed/interrupted work remains
  in Needs you until explicitly reviewed. Routine successful work appears in
  Recent work, without crowding the attention summary.
- Membership, current source conversation, private-agent ownership and Library
  source access are checked on reads and acknowledgments. Owners do not receive
  a workspace-wide private-work queue. Archived sources retain their audience.
- Previews contain a bounded saved title, agent/source labels and status, never
  business-action arguments, question field values or full generated output.
- Read state is personal and durable. Reading never answers, approves, retries,
  cancels or completes work. Reading a pending request does not remove it.
  Explicitly reviewing a failed outcome only acknowledges that outcome.

## Implementation order

1. Inspect existing source checks and save desktop/phone OpenPencil concepts.
2. Add schema 25 for per-person typed source/version read/review receipts. Derive
   inbox entries from saved questions, actions and terminal jobs rather than
   creating a second dispatch or notification queue.
3. Add bounded cursor reads and exact source/version acknowledgments behind the
   existing session boundary. Reuse authoritative source checks, with filtering
   before pagination so inaccessible work cannot hide older visible requests.
4. Add React Home summary and inbox, current-source invalidation, stale-read and
   duplicate-submit guards, keyboard/focus behavior and explicit private-runtime
   entry. Keep approval history available in Chat.
5. Verify personal isolation, forged identity/version denial, revoked source and
   Library access, pending-request persistence after read, terminal review,
   pagination, restart and real Goose question/approval continuation. Retain
   browser captures and process evidence; publish one dependent focused PR.

Pages contain at most 25 entries and an authorized next cursor. The summary uses
the first page, with a bounded count rather than an unverified global total.
No exact hidden-work count or inaccessible source identity is returned.

## API, storage and lifecycle

`GET /api/identity/workspaces/{w}/attention?view=needs_you|recent` returns
`{available,items,next_before}`. Pass the opaque `next_before` value as `before`
for the next page. Cursor ties use saved time, source kind and source ID. This is
a live current-access view, not a frozen multi-page business-history snapshot.
The Home summary displays at most 25 entries with a `+` when another authorized
page exists. Only its first three entries are previewed.

`POST` to the same route accepts `{kind,id,version,reviewed:false}`. An exact
current source/version is required even for a repeated acknowledgment. This
monotonic write marks the entry read; it does not mark a chat message seen.
`reviewed:true` is supported only for failed/interrupted terminal outcomes.
Concurrent and repeated writes preserve the original read/review receipt.
Question answers and approval decisions retain their separate original APIs.

Schema 25 (`m20261026_000025_workspace_attention`) adds
`relay_attention_receipts`, keyed by workspace/person/source kind/source ID.
Versions separate pending approvals from unconfirmed outcomes, and terminal
outcomes from each other. Receipts are removed with workspace membership.
No notification write initiates inference or execution. Stale executing actions
older than the existing 60-second confirmation window appear as unconfirmed;
reading cannot retry, reconcile or confirm them.

Apply the migration with the administrator workflow in
[PostgreSQL setup](postgres-workspaces.md), then reapply runtime grants. Startup
never migrates automatically; the current full schema requirement is listed in
[PostgreSQL setup](postgres-workspaces.md). Only disposable test
schemas were migrated here. This increment introduced schema 25. The inactive AWS manifest's
activation controller/executor counts remain zero and activation autoscaling
remains disabled.

The React summary and inbox share the existing live source-access transport.
One visible projection polls every five seconds; summary polling pauses while
the inbox is open, and inbox polling pauses during exact review. Access/reset
events still clear affected data while paused. Aborted/late reads cannot replace
another tab/page or survive unmount. A failed projection read clears titles.
The optional Home feature loads on demand and has its own error boundary, so a
missing chunk does not remove Home panels or navigation. No new dependency was
added. The production entry chunk is 494.73 kB versus 493.86 kB at the previous
milestone; the attention feature is a separate chunk.

## Local verification

- 95 Rust unit tests; 162 standalone PostgreSQL tests, including seven new
  attention checks. These cover v24 upgrade/repeated migration, question/approval
  authority separation, unconfirmed writes, read/review concurrency and restart,
  designated approvers without request grants, 53 tied-time terminal outcomes,
  foreign workspaces and 27 newer Library records revoked before pagination.
- 132 web behavior tests, including seven new attention journeys, plus retained
  Home panel/order/native-inbox compatibility. Type checking, browser-test type
  checking, production builds and warning-denying Clippy pass.
- 52 existing local Python checks, including schema-25 inactive preflight
  compatibility. No actual AWS service was contacted.
- Executed Chromium with real pinned Goose and a scripted local model: Home
  question read/answer, separate exact approval, one authenticated MCP write,
  source-thread result navigation, second session/controller restart, explicit
  failed-outcome review without retry, mobile/dark layout, keyboard focus return
  and live source revocation. No paid model was used.
- Five retained process/browser journeys pass for shared questions, the isolated
  controller-local question adapter, archived task continuation and managed/native
  saved readers. All 709 checked source files are within 500 lines; 98 changed
  documentation links resolve.

Evidence is retained in `.tmp/workspace-attention/`; editable OpenPencil concepts
and inspected application captures are in
[designs/workspace-attention](../designs/workspace-attention/README.md).
The initial broad ignored-test command also selected an unrelated S3-emulator
test without its loopback fixture; that failure is retained. The correctly scoped
standalone PostgreSQL command passes and does not claim S3 acceptance.

Reproduce with `cargo test --locked --lib`, `npm test --prefix web`, and
`RELAY_TEST_DATABASE_URL=... cargo test --locked --lib identity::postgres:: --
--ignored --skip embedded_gateway_uses_controller_owned_credentials`. Run
`node --test tests/workspace_attention.mjs` with the same disposable database,
built binary/client and installed pinned Goose executables. Its database name
must begin `relay_simulation_`; it creates a new isolated schema and retains
process/browser records.

## Remaining scope

Generic App notification registration, preferences/DND, email/push delivery,
automatic retries and universal native/shared attention parity remain separate.
The existing native runtime inbox and Chat approval history are preserved; this
slice does not claim a universal notification SDK or external delivery SLA.

# Current conversation context preview

Agents → Knowledge → **Preview conversation context** opens a read-only drawer.
Choose an accessible conversation with that agent to inspect eligible recent
messages and approved notes without submitting a task or calling a model.

The recent-message and note sections remain a current projection. Separate working
summary and saved-task sections now expose [versioned context](versioned-conversation-context.md)
under their own current-access checks. This is not a complete raw prompt. Actual execution
adds its exact request/cutoff, current identity and authorized task sources. A
paused agent or archived channel can be inspected without enabling new work.

## Server boundary

`GET /api/identity/workspaces/{workspace}/agent-profiles/{agent}/context-preview?chat={conversation}`
authenticates the session and checks current workspace membership, agent access,
conversation visibility and agent participation. It does not accept another
requester, source identity or arbitrary scope. Agent maintainership or workspace
ownership does not grant access to another person's private conversation.

The preview reuses the execution history selector: managed profiles use up to 60
eligible messages, current since-joining policy and the existing serialized
14,000-byte history budget. One oversized final message can exceed that budget,
as in the execution selector. Source-dependent answers and Library source
questions/titles are excluded through the same conservative history filter.
A legacy direct profile uses 30 messages; a legacy channel profile must be saved
as a managed profile before this preview is available because no exact task/thread
was selected. No private title is needed in the projection.

Approved notes use the same selection as execution, only when knowledge is enabled.
The shared selector now includes stable note IDs, revisions and conversation scope,
orders creation-time ties by ID and counts newline separators in its 16,000-byte
budget. It retains the existing 20-note maximum and current forget filter. These
references also anchor saved task snapshots. The versioned-context contract defines
correction/revocation fencing and the limits of retention and historical reuse.

Reads use the existing workspace transaction boundary and a five-second SQL
statement timeout. They create no jobs, inference, approvals or audit mutations.
The current viewer and selected conversation are the only permitted audience.

## Client behavior

The drawer uses the existing dialog/focus/scrolling pattern. Messages are rendered
as plain text with a short excerpt and deliberate full-text expansion. Stored
Unix-second timestamps are converted for display, including machine-readable
`time` elements. Notes use stable record IDs. The preview describes joining policy,
knowledge-off, paused, archived, empty, loading and denied states explicitly.

Viewer/workspace identity, agent and conversation are bound to the stored UI state.
A render for another scope cannot display the old result or error, even before
effect cleanup. Changing conversations also aborts outstanding reads.
Closing removes listeners/timers and aborts work. Access/reset/source notifications
clear the cached projection before reloading. Visible previews refresh every ten
seconds and on focus; an explicit refresh also clears and reloads. Failed reads
clear content and retain the conversation choice for a retry. The preview does not
promise immediate removal from an offline screen or erase material already seen.

## Local evidence

Three PostgreSQL cases cover person/workspace/agent denial, ownership versus
conversation access, current joining/knowledge policy, history bounds, forgotten
notes, source-dependent exclusions and zero new jobs. UI tests cover explicit
selection, stale responses, failed/revoked reads, inert content and unmount aborts.
A committed-render regression changes the authenticated viewer while keeping the
same agent/conversation mounted and holding the new read; the previous person's
context must never appear. Ordinary account switching still reloads the document.

`tests/context_preview.mjs` exercises the actual browser/backend and real Goose
with local scripted inference. It checks preview inspection creates zero jobs or
model calls, an open revoked view clears, model input uses the same permitted note
selection, forgetting updates the projection, timestamps are current, long text
expands, focus returns and desktop/narrow/phone-dark layouts stay bounded.

The original current-preview slice required no migration. The later
[versioned-context implementation](versioned-conversation-context.md) adds schema 31,
protected snapshots, bounded cited summaries and correction/revocation fencing.
Cloud operation, semantic learning and complete retention/erasure remain unproven.

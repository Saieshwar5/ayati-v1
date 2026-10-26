# Agent job guidance and draft-only rehearsal

Implemented locally 26 October 2026, serially after Home attention PR #71.
The initial plan was saved before implementation. This is a job-guidance and
draft-only sample increment, not full guided activation or model-quality acceptance.

The existing profile saves responsibility, escalation and custom
instructions. Explicit **Successful outcome** and **Routine** fields now join that
versioned profile and its permanent Overview. These remain behavioral guidance;
they never become account, tool, budget or approval authority. Creation/editing
uses the current bounded editor and preserves its saved settings and visibility.

**Try a sample task** is available to current managed-agent maintainers. A person writes a sample,
reviews the profile version and optionally selects up to three readable Library
sources already assigned to the agent by each item's steward. Selection does not
grant, change sharing or copy a private source into normal channels. Existing
agent-wide approved notes are included only when enabled; conversation-only notes
are excluded because a rehearsal has no conversation audience.

## Execution contract

- Explicit Start saves a durable exact request receipt and returns promptly.
  Queued/running/final states are inspectable without another model request.
- Reuse the existing pinned Goose tool-free `Prepared` harness used by setup
  suggestions: explicit empty extensions, no control broker, VM, browser,
  computer, platform tools, shell, business accounts or autonomous MCP calls.
  The result is a draft answer, never a claim that a real action occurred.
- Saved profile/source versions describe the actual sample context. Sources are
  checked again before use, during execution, before storing output and on read.
  Revocation/withdrawal hides private content and stops publication. Profile
  edits do not rewrite an earlier sample; show the saved version clearly.
- Use an additive schema for personal rehearsal records, durable status, bounded
  output and ownership/lease fencing. Run through a small bounded controller-local
  tool-free worker, following the current setup-assistant precedent. This is not
  fleet/cloud capacity acceptance. Report unavailable model configuration.
- Deduplicate Start by exact actor/agent/request/payload. Repeated or lost
  acknowledgments must not trigger another inference. Stop cancels this sample
  only. Stale ownership cannot publish. Expired execution is interrupted and
  requires an explicit new attempt; never silently repeat paid inference.
- Own sample history remains personal; an Owner/maintainer cannot inspect another
  person's selected private sources. No chat job, approval or business effect is
  created. Sampling neither activates grants nor pauses/changes the actual agent.

## Saved state and API

Schema **26**, `m20261026_000026_agent_rehearsals`, adds personal sample receipts
and source-scoped live invalidations. Apply migrations and runtime grants before
startup, as described in [PostgreSQL setup](postgres-workspaces.md). Existing job
settings and question/attention records survive the populated-schema upgrade.
Older profile clients retain the optional guidance fields when omitting them;
an explicitly empty field clears it. Each accepts up to 2,000 bytes of plain text.

Under `/api/identity/workspaces/{w}/agent-profiles/{a}`:

- `GET /rehearsal-options?search=...` returns the current saved job, up to 50
  eligible assigned sources, note metadata, personal active sample ID/state and
  whether the controller's model configuration is available.
- `GET /rehearsals?before=...` returns up to 25 personal history entries and an
  opaque cursor. `POST /rehearsals` accepts an exact ID, profile revision, task
  up to 4,000 bytes and up to three source ID/revision/access-revision references.
- `GET /rehearsals/{id}` returns compact saved context/state/output references.
  `GET /rehearsals/{id}/response` uses the shared exact reader/Markdown download.
- `POST /rehearsals/{id}/stop` returns only ID/state, so a requester can stop their
  own active sample without disclosing a withdrawn source. Terminal records do
  not become runnable again.

Every read/write authorizes current membership and agent maintainership; private
agent samples remain private even from a workspace Owner. Source versions and
approved-note revisions are checked before claim, at heartbeat, before publication
and on every disclosure. Personal `sample_access` live events contain no resource
IDs, titles or output and clear open sample/reader state while access revalidates.
Library/note changes immediately invalidate matching queued/running work. Member
removal or role changes invalidate active samples while retaining the exact
receipt across rejoin. This prevents old Start acknowledgments from running again.

The queue permits one active sample per actor/agent and ten per workspace.
Database-coordinated admission permits at most two live sample leases across
controllers. Execution is limited to 45 seconds, with a 15-second lease and
one-second authority/ownership checks. Expired leases become interrupted;
graceful shutdown cancels/drains owned processes. Neither path automatically
repeats inference. Prepared context is capped at 256 KiB and erased on terminal
state; output is capped at 64 KiB. Selected Library text uses short excerpts;
enabled agent-wide notes are bounded to 20 notes/16 KiB. Temporary execution
scratch is removed after each run.

Model availability is required only for new execution. Exact saved replay and
reading still work when the controller's model key is later unavailable. A
controller-local helper follows the existing setup-suggestion execution boundary;
this does not establish cloud/fleet worker isolation or capacity.

## Verification and design

Editable OpenPencil desktop/phone concepts precede the application UI. See
[the design record](../designs/agent-job-rehearsal/README.md) for inspected concepts
and actual local desktop/phone/dark captures.

`cargo test --locked --lib rehearsal -- --ignored`, with a disposable
`RELAY_TEST_DATABASE_URL`, covers exact replay, immutable context, cross-user and
private-agent denial, Library/note revocation, role/removal/rejoin, bounded shared
claims, stale publication, Stop, expiry, migration and older-client preservation.
`web/tests/agent-rehearsals.test.mjs` covers explicit review/Start, duplicate/failing
submissions, saved reads, source invalidation, stale responses and focus.

`node --test tests/agent_rehearsals.mjs` uses real pinned Goose against a scripted
local model with authenticated MCP fixtures. It checks asynchronous/reopened
state, exact Markdown bytes, narrow/dark UI, graceful and abrupt restart, expired
leases without automatic replay, absent model configuration, invented tool calls,
Stop and zero actual MCP/business operations. Normal questions, approvals,
attention, reading and Chat organization retain separate regression journeys.

The final local check passes 96 Rust unit, 172 standalone PostgreSQL, 140 web and
52 offline Python checks, Clippy, Rust/web builds, browser-test types and all
735 checked source files at or below 500 lines. Nine executed process/browser
journeys include both new sample scenarios and seven retained workflows. The
sample scenarios record one and five scripted model requests respectively and
zero MCP calls; opening or replaying samples adds none. Evidence and earlier
failed checks are retained in `.tmp/agent-job-rehearsal/verification.json`.

Normal agent chat retains its existing tool controls and source paths. A selected
Library source here is not automatic agent-wide knowledge retrieval in every
conversation. AI job drafting, generic ingestion, richer policy enforcement,
full guided activation, cloud execution, and nontechnical-user quality acceptance
remain separate. No provider/cloud spend is authorized by this local sprint.

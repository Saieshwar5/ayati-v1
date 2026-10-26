Managed workspace agents now support [durable personal Home panels](workspace-home-panels.md)
through PostgreSQL and the scoped control bridge. The contract below describes the
separate adopted local-owner runtime, including its maintenance and approval flow.

The [Organized Home refinement](organized-home.md) supersedes the older section
counts below with fixed, single-section templates for new writes and revisions.

# Agent-created Home panels

Home can submit a real request to an agent to create a persistent view. The
workspace MCP tools save typed text, metrics, tables and chart snapshots.
Arbitrary HTML and scripts are not accepted. Panels belong to their creating
agent; Home aggregates them for the trusted local owner.

Each saved revision is immutable. The owner reviews a new panel and chooses
Keep on Home. The last approved snapshot remains visible while a changed scope
awaits review. The panel shows its saved time, sources and maintenance status.
Agent findings without verified source receipts are labeled accordingly.

Successful connector executions and app observations from the current run are
attached automatically. Supplied source IDs must belong to that agent and retain
current account/operation or app/collection grants. Local revocation hides cached
panel content, fences maintenance and invalidates the agent's working context.
Audit history, visible conversation and durable memory are retained separately.

The owner can allow later updates of the same panel scope. Automatic acceptance
requires the exact expected revision, unchanged title/purpose and source scope,
an active reviewed panel, current grants and enabled updates. Otherwise the
revision needs review. Remote operation execution still needs its exact approval;
enabling panel updates does not grant unattended external tool authority.

An approved `schedule_create` action may bind the existing durable scheduler to
the panel. Pause/remove, disconnected sources and review-required changes pause
bound schedules and cancel queued occurrences. Active work is not automatically
undone. Failures preserve the last saved snapshot rather than inventing fresh
values. No independent panel scheduler or background provider poller is added.

`tests/home_panels.rs` checks ownership, source provenance, revision review and
revocation. `tests/home_panels.mjs` exercises real Goose creation, owner review,
safe rendering, scheduled refresh, pause and restart using synthetic sources.
Live business-data freshness and updated guest acceptance remain unverified.

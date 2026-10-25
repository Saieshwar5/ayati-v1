# Guided agent permission review

Open **Agents → Set up permissions** for an agent you maintain, or **Tools and
approvals → Permissions → Guided permission setup**. This extends the existing
[workspace Connections and delegation](workspace-connections.md) and
[shared-agent enforcement gateway](shared-agent-control.md). Current startup
requires PostgreSQL **schema 22**.

## Product flow

1. **Work and accounts:** describe the work this permission review should support,
   choose one to five eligible connected accounts, and select a work conversation.
   Owners approve/provision accounts in Settings; delegated members see only
   accounts available within their delegated scope. The review description is
   retained with the draft; it does not replace the agent's profile instructions.
2. **Review actions:** inspect all reviewed operations, their Owner-set limits and
   designated approvers. Set each operation to Allow automatically / Ask first /
   Block. Save without changing active permissions, close and reopen to continue.
   The review updates selected accounts in one conversation; other accounts retain
   their separately configured permissions.
3. **Preview and activate:** inspect every decision and optionally enter example
   fields. Preview performs no business operation. Explicitly confirm review, then
   activate the saved configuration. All selected rules change atomically and an
   immutable activation receipt records the person, draft, revision, scope and time.

An optional **Create work conversation** action creates an empty conversation
with the creator and listed approvers for a workspace agent. Private agents retain
their owner-only conversation boundary. Ask requires the designated approver to
be able to see both the agent and work conversation; setup cannot publish a
private transcript to solve missing access.

[Authorized approvers and channel delivery](channel-approvals.md) now add a
person/channel choice for each Ask operation. The original Owner remains the
default; a delegated creator may select another person explicitly authorized by
that Owner. The selected person also needs independent agent/source visibility.
[Durable approval waits](durable-approval-waits.md) now continue the original task
after a confirmed action outcome. Multiple named managers and backup reassignment
remain later work.

## Suggestions and complete coverage

Manual review starts with every operation blocked. **Use AI suggestions** is an
explicit opt-in: it sends only the description plus reviewed action identities,
titles, effects and maximum permissions to the agent's configured model. Normal
model usage may apply. Opening setup never calls a model.

Suggestions reuse the pinned Goose conversation runtime in a fresh private
directory, with no tools, MCP credential, browser, VM, memory, Library files,
conversation history or workspace extensions. Only the configured model provider
credential is forwarded through the existing isolated process configuration.
The initial implementation runs this bounded setup request on the controller,
with two concurrent suggestion slots and a 45-second execution deadline; it does
not add another cloud worker service. Temporary runtime files are cleaned up.

The response must be bounded JSON containing known action choices and warnings.
Malformed output, unknown identities, duplicate choices and executable-policy
fields are rejected. A suggestion cannot exceed the Owner's ceiling. An omitted
operation remains blocked. Human edits and explicit activation still determine
the resulting permissions. Model judgment/conditional interpretation needs real
provider evaluation; the local fixture proves orchestration and enforcement only.

Discovery enumerates the complete bounded remote tool list while checking the
reviewed operation schemas. Every listed tool is either mapped to a reviewed
operation or displayed as unsupported. Unsupported tools remain unavailable,
including tools omitted by the model. The UI exposes reviewed tool mappings for
inspection. Tool descriptions/annotations do not establish real-world meaning;
operators still review operation mappings and upstream credential scope.

Activation repeats discovery and compares the inventory, profile and credentials.
Changed reviewed schemas, newly discovered tools or changed configuration require
a fresh review. Inventory completeness is not a guarantee that a remote server
will never change behavior behind an unchanged schema.

Amount, time and individual-record conditions cannot be configured by this slice.
The UI says so. Natural-language descriptions and model warnings are guidance;
the enforced permissions remain the supported operation modes and conversation.

## Persistence and authority

Migration `m20261025_000020_guided_permissions` adds `relay_control_drafts` and its
lookup index. Existing permissions, jobs and action receipts are preserved. Drafts
are private to their author and agent/workspace, even from another Owner; all
reads and writes require current agent-maintenance access. They are server saved,
not browser-local authorization. A draft captures agent, account, delegation and
rule revisions, its complete choices and discovered inventory.

Manual saves, preview and activation share the same rule-validation function.
Real execution retains the gateway's live effective-rule checks. Preview evaluates
the draft's permission scope for its author and selected conversation, using the
real argument-schema validator; it is not a model/tool call or a guarantee of
future provider availability, job eligibility or external outcome.

Edits require an expected revision and clear the previous preview. Exact edit
retries return the same saved version; stale different content fails. Activation
requires a preview of the current saved revision, explicit confirmation, and fresh
membership/agent/account/delegation/rule checks under the workspace transaction.
One stale rule prevents the entire activation. No partial activation is published.

Membership or role changes clear affected authors' and approvers' saved previews.
Removal/rejoining cannot reuse a previous preview even if a newly issued grant
reuses the same revision number; the person must preview again under current access.

Activation retries return the original receipt and never rewrite rules. A later
revocation is not undone by replaying an old activation. Reducing a maintained
agent's operation to Block requires no grant to execute that operation; raising it
still requires current delegation. Existing pending approvals affected by changed
rules are invalidated through the gateway's established path.

Routes beneath `/api/identity/workspaces/{w}/agent-profiles/{agent}/setup`:

| Route | Purpose |
| --- | --- |
| `GET /` | Eligible context and the author's latest saved review |
| `POST /drafts` | Discover accounts and create a manual or suggested draft |
| `POST /drafts/{id}` | Save choices with an expected revision |
| `POST /drafts/{id}/preview` | Check decisions and optional example fields |
| `POST /drafts/{id}/activate` | Verify inventory and atomically activate |

## Upgrade and verification

Back up existing identity data and stop old binaries before explicit administrator
`workspace-db migrate`. Reapply runtime table/sequence grants using the existing
deployment procedure. API startup checks the full schema and never migrates it.
Ordinary workspace data was not migrated by this implementation.

Focused checks are `identity::postgres::setup_tests`, the setup assistant's Rust
unit tests, `web/tests/guided-setup.test.mjs` and `tests/guided_permissions.mjs`.
The process test requires a dedicated local `relay_simulation_*` PostgreSQL
database via `RELAY_TEST_DATABASE_URL`. It uses real Goose, a scripted local model,
an authenticated MCP fixture with independently stored records and Playwright.
It covers AI omission/default block, full coverage, tool-free suggestions, saved
draft reload, preview without business effects, atomic activation, actual
Allow/Ask/Block execution, exact retry, inventory changes, restart and credential
filtering. Prior Connections/shared-control journeys also pass as regressions.

The implementation was verified locally with 84 Rust unit tests, 108 disposable
PostgreSQL tests, 92 web tests, 52 Python tests and four process journeys. This is
local workflow/protocol evidence, not live model quality, customer OAuth, a real
commercial MCP provider, release-image/AWS acceptance or billing readiness.

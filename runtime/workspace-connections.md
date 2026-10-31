# Workspace connections and delegated agent setup

This increment makes the existing shared-agent control engine configurable by an
Admin and explicitly delegated members. It requires PostgreSQL **schema 22**.
It provides reviewed, operator-configured MCP accounts; it does not yet provide
self-service OAuth or arbitrary custom-MCP registration. The subsequent
[guided review](guided-permissions.md) adds optional AI permission drafts.

## Admin journey

Open **Settings → Workspace → Connections**. The page separates three decisions:

1. **Approve system** makes a configured system/account eligible for the workspace.
   It does not connect the account or grant an agent permission.
2. **Connect account** verifies authentication and the reviewed operation schemas
   without executing a business operation. Credentials remain on the controller.
3. **Manage access** selects a workspace User and the maximum permission they may
   assign for each operation on agents they maintain. New operations are unavailable.

The operation limits are **May allow automatically**, **Must ask an authorized approver** and
**Unavailable**. The first permits Allow/Ask/Block; the second permits Ask/Block;
the third permits no execution. The admin saving a delegation becomes its default approver
when the member configures an Ask rule. [Scoped approval grants](channel-approvals.md)
now allow that admin to authorize other people for selection. The page displays the current approver.

Since schema 45 everyone may create agents; the former **Agent creators** section is removed.
Removing creation permission prevents new workspace agents; existing agents remain
maintainable. Members retain private-agent creation. Neither creation right grants
tool access, access to another person's private agent, or business approval rights.

Connections can be disconnected or their system approval removed. These operations
disable access, block affected agent rules/delegations and invalidate pending
approvals. Missing/broken operator configuration does not hide revocation controls.

## Delegated member journey

An authorized member creates a workspace agent through the existing Agents wizard.
Open **Tools and approvals → Permissions**, select a conversation and save each
operation as Allow automatically, Ask first or Block within its delegated limit.
The UI names the admin who receives Ask requests. The server enforces the same
limit independently of the UI. An Ask rule requires that admin to be able to see
the agent and selected conversation; it cannot expose a private conversation.

The member must maintain the agent to edit its tool rules. This increment uses
the existing agent owner/maintainer plus workspace admin model; separate named
multi-person management grants are not implemented. A workspace admin may manage
workspace agents, but cannot inspect another person's private agent.

Activity retains the existing exact-action approval/result UI. Only the designated,
currently authorized person can approve that action. Other admins, the agent creator and ordinary channel
participants cannot substitute for that person. Approval dispatches the saved
action. Channel request cards, selected eligible people/destinations and a restricted inbox
are implemented in [channel approvals](channel-approvals.md). [Durable approval waits](durable-approval-waits.md)
now reconstruct the original task after a confirmed outcome, including on another worker.

## Enforcement and persistence

Migration 19 adds workspace system approvals, creator rights, operation delegation
ceilings, and each rule's configuring person/delegation revision. Existing admin
connections are backfilled as approved; existing rules retain their mode/revision
and attribute configuration to their recorded approver. Members receive no new
creation or delegation grants during migration.

Runtime discovery, admission, approval claim and final dispatch read
`relay_control_effective_rules`. Its live checks include current membership,
agent ownership, admin approver status, approved/enabled account, saved operation
mode and current delegation revision/ceiling. These are not prompt instructions.
Requester, conversation, Library source and job/lease checks remain in the gateway.

Changing a delegation blocks that member's affected agent rules until reviewed
again, including when increasing the limit. Reconnecting/reviewing an account
blocks all its old rules and delegations. Membership removal or role change fences
related rules/delegations and pending actions; later rejoining/promotion cannot
revive those grants. Revocation cannot undo an already dispatched external effect.

Settings writes carry expected revisions. An exact retry may return success
without applying the change again; a stale different payload fails. Connection
verification runs outside the workspace transaction, then rechecks authority and
revision before saving. Every authorization mutation writes an audit record.

Workspace admin endpoints under `/api/identity/workspaces/{workspace}/control`:

| Endpoint | Purpose |
| --- | --- |
| `GET /connections` | Reviewed catalog, account state, people and delegations |
| `POST /systems` | Approve/revoke a configured system/account |
| `POST /connections` | Verify/connect or disconnect an account |
| `POST /delegations` | Save one person's operation ceiling and responsible admin |

Agent rule writes also carry `delegation_revision` for delegated Users. Agent
snapshots expose only eligible or already visible accounts and no operator catalog.
The older admin agent-connection endpoint remains compatible but now requires
workspace approval; the UI uses the workspace endpoints above.

## Upgrade and operator setup

Back up the identity database and stop old binaries before explicitly applying
`workspace-db migrate` with the administrator connection. Reapply runtime grants,
including SELECT on the new view, using the existing PostgreSQL deployment procedure.
Startup requires all 22 migrations. Do not run an old binary against this schema.
This change was verified on disposable databases; the ordinary workspace database
was not migrated.

Profiles still use controller-only `AYATI_MCP_SERVERS_FILE` and private bearer files.
See [shared-agent control](shared-agent-control.md) for transport/schema limits and
[the example profile](../integrations/remote_mcp/servers.example.json). An optional
`account` field supplies a non-secret display label. That label is not proof of
remote account identity; the operator must verify credential/account scope. Use a
distinct profile ID for each account, including accounts from the same provider.
Authentication/operation changes require review and never inherit old grants.

The following [guided permission review](guided-permissions.md) increment now adds
saved drafts, optional tool-free AI suggestions, complete coverage, previews and
atomic activation. Richer conditions, channel approvals,
durable continuation, customer OAuth, browser/computer controls and commercial
usage accounting are separate work.

## Verification

Run the normal Rust library suite, disposable PostgreSQL suite and web tests/build.
The live local journeys are `tests/workspace_connections.mjs` and
`tests/shared_agent_control.mjs`, using `RELAY_TEST_DATABASE_URL` for a dedicated
`relay_simulation_*` database. They use real pinned Goose, a scripted local model,
an authenticated local MCP server with independent records, and Playwright.

The delegated journey covers admin setup in Settings, member creation in Agents,
Allow/Ask/default-Block, API escalation denial, exact designated-admin approval,
UI revocation, controller restart, broken configuration and credential filtering.
Database tests additionally cover stale revisions, cross-workspace access, private
approver visibility, final dispatch revocation, role/rejoin revocation and a
populated schema-18 upgrade. Web tests cover failed drafts, double submission,
late reads, access loss and disabled permission choices. Screenshots cover
desktop/phone and light/dark settings.

This is local product/protocol evidence. Live commercial providers, real model
quality, cloud isolation, launch capacity and billing acceptance remain unproven.

Verified locally on 2026-10-25: 82 Rust unit tests, 102 PostgreSQL tests,
87 client behavior tests, 52 Python tests and three process journeys pass.
Clippy with warnings denied, formatting, 583-file line checks, production build,
browser-test type checks and 95 local documentation links pass. Reports and logs
are retained in `.tmp/workspace-connections/`; the final delegated browser journey
is `.tmp/independent-worker-IZgLSD`. The disposable PostgreSQL instance and fixture
processes were stopped after verification. Failed attempts remain separate from
the final passing evidence.

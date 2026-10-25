# Authorized approvers and channel delivery

This increment adds explicit account-action approval authority, per-operation
approver/channel selection, saved channel request cards and a restricted **My
approvals** inbox. It extends [guided permission review](guided-permissions.md)
and the [shared action gateway](shared-agent-control.md). Current startup requires
PostgreSQL **schema 21**.

## Owner and creator flow

In **Settings → Workspace → Connections → Manage access**, choose a workspace
User. **Agent setup limits** retain the existing Allow/Ask/Unavailable ceilings.
The separate **Business approval authority** section explicitly authorizes that
person to approve individual operations on this account. Each grant is saved,
revisioned and audited. It grants no agent-management or source-data permission.

An approver also needs independent access to the source conversation and data.
Workspace-agent approval can be reviewed without permission to request agent work;
these are separate rights. A private agent remains visible only to its owner.
Approval authority alone never exposes a private conversation or Library source.
Owners can be designated under their existing administrative authority.

An agent maintainer chooses **Ask first**, an eligible approver and a delivery
channel in guided review or manual operation permissions. The default remains
the Owner who granted the creator's setup delegation and the work conversation.
Delegated creators may select that Owner or another person explicitly authorized
by that same Owner. Authorization from a different Owner cannot expand their
delegation. Owners configuring rules directly may select any currently eligible
approver. The model and task requester cannot choose a substitute at runtime.

The selected person must be able to view the agent, source work and destination.
An alternate destination must be an accessible internal channel/group whose
human audience is contained within the source audience. A public destination
cannot receive a private-source request. Private agents keep approvals in their
private work conversation. These checks run at save/preview/activation and again
when a request is delivered. Changing selection changes the rule revision and
invalidates pending approvals affected by that rule.

## Human review

When the agent proposes an Ask operation, the controller saves the immutable
action and one delivery record in the same transaction. The configured channel
gets an **Approval request** card. Open **Review saved request** to fetch the
exact account, operation, arguments, requester, source and designated approver.
Only that currently authorized person receives decision controls:

- **Approve this action:** dispatch the saved operation through the existing
  current-authority, schema and outcome checks.
- **Reject:** save the rejection without executing the operation.
- **Request changes:** record that a revised proposal is required, with no dispatch.
  The original fields cannot be edited into an approved action.

**Chat → My approvals** opens the restricted inbox, with waiting requests first
and at most 50 recent candidates. Channel and inbox views refer to the same action;
opening a view, duplicate delivery, repeated clicks or controller restart cannot
create a second write. A different decision after a saved decision conflicts.
Ordinary chat text, mentions and reactions do not approve an action.

Request changes currently requires asking for a new proposal in the work
conversation. Automatic revision workflows, backup reassignment and resuming the
original Goose task remain later increments. Approving a saved action still uses
the existing separate bounded dispatch after the original proposal job finishes.

## Private data stays with its source

Channel messages store only generic text and a saved-action reference. Exact
arguments, private previews and results are never copied into message content.
History and live SSE payloads project the reference under the viewer's current
source, agent and Library access. An unauthorized channel member sees a restricted
card with no action identity or exact fields; the details API also denies access.

The review view rechecks access while open and clears its fields when access is
revoked. A newly added channel member cannot obtain earlier private arguments.
Workspace Owner status does not override private-source or Library visibility.
Library-bearing jobs retain the existing requester-only publication contract;
they cannot be declassified by appointing an approver or adding channel members.

If a previously valid destination becomes too broad or inaccessible, no channel
card is posted there. The delivery records a routing issue and the request remains
available in the designated person's restricted inbox **only while that person
can independently view the source data**. If source access is also unavailable,
the request cannot be decided. There is no automatic fallback to another Owner.

An action status update updates the existing card through the durable message
change stream. The card remains generic; current detail access is checked again
when opened. Executing actions whose result is overdue retain the existing
uncertain-outcome behavior and write hold.

## Stored authority and delivery

Migration `m20261025_000021_channel_approvals` adds:

- `relay_control_approvers`: scoped person/operation grants, their issuing Owner
  and monotonic revision. Records survive removal as disabled grants, preventing
  removal/rejoin from reusing an earlier authorization version.
- Rule bindings for the selected approver grant revision and optional channel.
- An action's selected delivery channel and `relay_control_deliveries`, with one
  durable delivery per exact action.
- An action-update trigger that refreshes its existing message reference.
- A revised `relay_control_effective_rules` view requiring current configuring
  authority, delegation, selected approver authority, account and revisions.

Reconnection/review disables existing approval grants as well as agent rules and
delegation ceilings. Changing an approval grant blocks affected rules, invalidates
pending actions and clears setup previews. Membership/role changes disable grants
issued to or by the affected person. Re-promotion, rejoining or reauthorization
cannot revive earlier pending approvals. Existing Owner rules migrate with their
current person and default source delivery; existing saved actions are not
retroactively broadcast to channels.

Approval admission and final dispatch recheck the selected person's membership,
grant, private-agent boundary, source conversation and Library access. Approval
review does not require permission to start agent work; the original requester
retains the task's independent request authority. Workspace
transactions serialize these checks with authority changes. Revocation cannot
undo an external effect already dispatched; uncertain writes retain their receipts
and are never retried automatically.

New workspace endpoints under `/api/identity/workspaces/{w}/control`:

| Endpoint | Purpose |
| --- | --- |
| `POST /approvers` | Owner saves one scoped approval grant |
| `GET /approvals` | The current person's authorized inbox projection |
| `GET /actions/{id}` | Current-authority exact request details |
| `POST /actions/{id}/decision` | Decide through the existing action gateway |

Manual rules and typed guided choices accept optional `approver`,
`approver_revision` and `approval_chat`. The controller derives defaults and
validates selections; machine tool requests cannot set these fields. Approval
authority remains separate from workspace roles, request permission, management,
channel membership and the ability to configure agent tools.

## Verification and upgrade

Back up identity data, stop old binaries, explicitly apply `workspace-db migrate`
with an administrator connection and reapply runtime table/sequence grants using
the existing deployment procedure. Startup checks all 21 migrations and never
migrates ordinary workspace data automatically. This implementation was tested
only against disposable databases; the ordinary workspace database was not migrated.

`tests/channel_approvals.mjs` uses real pinned Goose, a scripted local model, an
authenticated MCP test system with independent business records, PostgreSQL and
four browser identities. It proves Owner authorization, delegated selection,
exact non-Owner approval, channel-only observer denial, restricted inbox decisions,
request changes, revocation, restart and removal of open private details. Database
tests cover audience changes, Library sources, cross-workspace routes, other-Owner
delegation bypass, reauthorization, wrong people and final-dispatch access loss.
Web tests cover restricted cards, zero-effect opening, double submission, exact
retry, stale responses and clearing details after denial.

These are local workflow/protocol results. General choice/form messages for every
agent, legacy local-agent adapters, original-task continuation, complete inbox
pagination, commercial provider acceptance, billing and cloud acceptance remain
separate work. No new dependency, paid inference or cloud resource was introduced.

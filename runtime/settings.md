# Shared Settings and workspace ownership

Cuckoo provides workspaces for one or many people. Option B in
[the approved design](../designs/settings/README.md) is the current Settings reference.

Everyone has Settings → Personal: My account, Appearance and Help & feedback.
Only admins see Workspace: General, People, Access, Connections, Activity and
Teams. The global rail contains Home, Chat, Agents, Library and Settings.
People discovery remains in Chat; membership administration lives in Settings.
Account-menu workspace switching and sign-out remain available to everyone.

`?view=people` / `users` open Settings → Workspace → People for admins.
`?view=company` opens Workspace → Teams. `?view=feedback` opens Personal → Help &
feedback, preserving conversation-scoped reports. Shared agents are managed in
Agents; internal work records appear on Home. No workspace records are deleted.

## Roles and authority

Membership roles are Admin and Member (schema 43, D231; earlier schemas named them
Admin and User). New invitations always offer Member access; an existing admin can
promote a joined member. Admins have equal workspace administration authority,
including changing another admin to member or removing them. At least one admin
must remain. PostgreSQL serializes these changes with the
workspace row lock; SQLite uses its existing connection mutex and transaction.

An invitation with unconfirmed email delivery cannot grant access or be silently
sent again. An admin must cancel it before sending another invitation to that
address in the same workspace. The failure keeps the email draft and refreshes
People's invitation list immediately, labelled "Delivery unconfirmed". Cancellation
removes the local membership-grant path even if provider revocation fails.

Membership, settings, activity and invitation APIs enforce admin access. Chat's
member directory remains available to workspace participants without exposing
invitations. Agent, App and panel grants remain explicit and separate from roles.
Removal preserves shared history and attribution, removes membership/resource grants
and App sessions, and prevents accepted-invitation replay. Role changes invalidate
issued App sessions and affected business-operation rules/delegations. Existing server checks fence queued replies/results after
revocation. The client also rechecks its identity/role every 15 seconds and discards
the previous view on changes; server enforcement does not wait for that refresh.

General name updates include `previous_name` so stale saves fail. Activity lists
the latest 100 records for the current workspace, only for admins. Appearance keeps
its existing device-local persistence. Workspace credentials and provider secrets
remain server-side. PostgreSQL admins now use [Workspace Connections](workspace-connections.md) to
approve operator-configured accounts, verify them and delegate bounded agent setup.
Admins can also authorize [account-action approvers](channel-approvals.md) independently
from agent setup and source visibility. Self-service account registration/OAuth remains unavailable. Local connector management is rendered in React with the existing request contracts.
Agent details retains its existing legacy connector integration.

## Existing installations

Back up identity data and stop the old server before upgrading. Do not reopen an
upgraded identity database with an old binary.

PostgreSQL requires explicit migration 8 using an administrator connection:
`cargo run --locked -- workspace-db migrate` (or `./tools/local-postgres.sh migrate`
for this machine's local setup). API startup checks the schema and never migrates it.
Before migrating, review `SELECT role,count(*) FROM relay_members GROUP BY role`.
Schema 8 turned the first Admin/Member roles into admin/User without promoting
anyone; schema 43 renames admin to Admin and User to Member, again changing no one's
access. Pending invitations become Member invitations. Both migrations record each
conversion in the audit. Schema 43 switches off the members table's role-change
triggers for the rename only, so schedules, event triggers, hand-offs, samples and
saved context are not blocked by a change that grants or removes nothing.

The historical browser-only access preview still demonstrates the older three-role
model and is not connected to membership APIs. The existing adopted-installation
fence also remains: promoting a workspace admin does not expose the original
person's private legacy runtime. Shared workspace management follows the two-role
model; private-runtime migration is a separate operation.

## Verification

Real PostgreSQL tests cover owner-only reads and writes, promotion/demotion,
last-owner protection, stale name saves, simultaneous ownership removal and migration of
populated legacy memberships, invitations and dependent grants. React tests cover
role-based sections, failed saves, invitation retry, grants and double submission.
A separate scripted workspace was used for the browser journey, including admin
and User views, promotion/demotion while open, personal themes, general/activity,
short windows and narrow layouts. No model calls or invitation emails were sent.

See [the workspace scenario audit](workspace-simulation-2026-10-22.md) for the
subsequent 20-person workload, provider-fixture tests and recovery/restore results.

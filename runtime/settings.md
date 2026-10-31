# Shared Settings and workspace ownership

Cuckoo provides workspaces for one or many people. Settings follows the
[simplified design](../designs/settings-simplified/README.md) (D232, 2026-10-30):
one sidebar with two groups.

| Group | Members | Admins |
| --- | --- | --- |
| You | Profile, Preferences, Help | Profile, Preferences, Help |
| Workspace | This workspace | Workspace, People, Connections, Usage, Billing, Audit log |

- **This workspace** (members):
  - the workspace name, their role, and how many people and agents they can see,
    read from the member directory;
  - the admins to ask, and what admins can see (only public channels and the ones
    they are in; never other people's private channels or direct messages);
  - **Leave workspace.** The person chooses who takes over their shared agents and
    Library items (an admin by default) and sees the same hand-over summary as a
    removal. The page then reloads to the workspace chooser. The last admin is
    refused, so an admin makes someone else admin and becomes a member first.
- **People** (admins):
  - roles on each row, with a ⋯ menu: Make admin / Make member, Agent access…,
    Remove from workspace…;
  - the only admin can't be made a member or removed, and admins can't remove
    themselves here;
  - removal opens the hand-over dialog described below.
- **Usage and Billing:** "Coming later" placeholders.
- **Audit log:** formerly Activity. Rows read "time · who · what they did" as
  sentences with names, with filters All, People, Connections, Approvals and Admin
  views (admin views are highlighted), 50 at a time with **Show older**. In
  simulations it also shows the simulation results; the Teams page was removed,
  and the team records remain on the server.

## Settings completed (schema 49, M5)

- **Preferences:** theme stays on the device. Notification defaults are stored on
  the server per person (`relay_member_preferences`) and save as you go: desktop
  alerts on/off, the default bell level for channels without their own, and
  alerts when an agent you manage asks for approval. Direct messages always
  notify. Email alerts are "Coming later". `GET/POST /preferences`; the defaults
  also come back with `/chat-notifications`.
- **Workspace** (admins): name; colour; time zone (used by Home's clock, which
  shows device time until one is chosen); approval backup delay (1 hour to 7
  days, default 4 hours); **Models**: the provider/model pairs agents may use and
  the default for new agents. An empty list allows any model. An agent can only
  switch to an allowed model, and an agent whose model the admin removed refuses
  new requests until its owner picks another. `GET /settings`,
  `POST /settings/workspace`.
- **Delete workspace:** the admin types the name; deletion is scheduled for 7 days
  later and every member sees a banner. Any admin can undo until then
  (`POST /settings/deletion`, `POST /settings/deletion/cancel`). When it is due, the
  cleanup job closes the workspace: nobody can open it, it leaves everyone's list,
  and its agents, schedules and triggers stop. **Not yet built:** erasing the
  stored records and Library files permanently; that needs its own design because
  many records are append-only. Export data is "Coming later".
- **Help:** **Report a problem** replaces the internal bug tracker (its page is
  removed; its server records stay). A report has a topic, what happened and,
  if ticked, browser and page facts (never message contents). Reports go to the
  Cuckoo team, not the workspace: operators list them with
  `ayati workspace-db problem-reports`. Short guides link to People, Agents,
  Connections and Activity.
- **Phones:** the sidebar list comes first, and a chosen page shows "‹ Settings".

Pages use `?view=settings&section=<page>`. Older links still open the right page:
- `scope=…&section=my-account|appearance|help-&-feedback|general|access|activity|teams`;
- `?view=people|users|company|feedback`;
- `#theme`.

Workspace pages fall back to Profile for members, and the server refuses their APIs
regardless. Account-menu workspace switching and sign-out remain available to
everyone. No workspace records are deleted.

## Pause all agents (schema 51)

An admin's emergency switch, in **Activity → Now** and **Settings → Workspace**.

- **Pausing** asks for an optional reason (everyone sees it), then in one
  transaction stops every unfinished request with the same stop as a person's
  Stop: running attempts end at their executor's next lease renewal, pending
  approvals are invalidated, and replies read "Stopped because all agents were
  paused." One audit entry records who, how many requests and why.
- **While paused:** sending to an agent is refused with HTTP 409 and that reason;
  person-to-person messages still work. Schedules **skip** a due run and stay on
  (a one-off waits for its person to run it again). Trigger events that arrive are
  not handled and say so on the delivery. An approval decided just before the
  pause is not sent. Library questions to agents are refused too.
- **Every member** sees a banner with who paused, when and why; it arrives live
  through the change stream (an `agents` event), so open tabs need no reload.
  Admins can resume from the banner.
- **Resuming** lets agents take work again. Stopped requests don't restart by
  themselves, and agents paused on their own stay paused.
- The pause is a separate gate from agent access on purpose: lost access
  permanently blocks schedules and triggers, and a pause must not.
- API: `GET /agents/pause` (any member; the stop count only for admins),
  `POST /agents/pause` `{note}` and `POST /agents/resume` (admins).
- Checks: `settings::pause` PostgreSQL tests, `web/tests/agents-pause.test.mjs`,
  and `tests/agents_pause.mjs` with real Goose.

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

## Removing a person

`GET /api/identity/workspaces/{w}/members/{user}/responsibilities` (admins only)
lists what removal would affect:
- shared agents they own, and private agents that stay unused;
- workspace-wide Library items they look after, and private or selected items
  that stay as they are;
- enabled schedules and event triggers they authorized;
- approvals waiting for them;
- active tool rules they set up or approve;
- invitations they sent.

Removing them (`POST …/members/{user}` with `{"role": null, "handover_to": <id>}`)
does all of the following in one transaction. `handover_to` defaults to the acting
admin and must be another current member.
- **Moves** their shared agents and their workspace-wide Library items to that
  person. Private agents, private pages and items shared with selected people stay
  as they are, because moving them would show the new owner something it was never
  shared with. A selected editor can still take such an item over.
- **Cancels** invitations they sent, and revokes them at the provider after commit
  (best effort, as for a manual cancel).
- **Keeps the existing safety rules:**
  - waiting approvals are cancelled ("request a new action"), because an approval
    belongs to the person authorized to give it;
  - tool rules they set up or approve switch to Block;
  - their schedules and event triggers are paused for review, because a saved
    routine runs on its creator's authority and cannot be handed on.
- **Records one readable audit sentence** (schema 44, `relay_audit.summary`), for
  example "Arjun Mehta removed Ravi Kumar; 2 agents and 1 Library item moved to
  Meera Iyer; 2 schedules paused for review; 1 invitation cancelled". Role changes
  record "… made … an admin" or "… a member". The activity list shows the sentence
  when one exists.

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

`tests/settings_people.mjs` is the browser journey for the sidebar layout: the admin
and member sidebars, the People row menu, removal with hand-over and its Audit log
sentence, the phone list/page layout and dark mode, with screenshots
(`SETTINGS_SHOTS=<dir>` saves them elsewhere).

See [the workspace scenario audit](workspace-simulation-2026-10-22.md) for the
subsequent 20-person workload, provider-fixture tests and recovery/restore results.

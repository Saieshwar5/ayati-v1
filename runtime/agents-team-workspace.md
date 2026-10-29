# Agents team workspace

The Agents screen implements the selected **B / Agent workspace** OpenPencil
concept. A searchable team roster stays beside the selected profile on desktop.
On phones, selecting a person opens their profile; **Agents** returns to the
roster. The quiet rail and single rounded workspace surface follow the
[Relay design system](design-system.md). Editable design and checked captures
are in [the design record](../designs/agents-workspace/README.md).

## What a person can do

- See the agent's actual responsibility, responsible person, visibility and
  whether it accepts requests. Availability is not a service-level promise.
- Use Overview, Work, Knowledge, Access, Automation, Environment and Activity
  tabs. Selection and tab preferences are scoped to the current user/workspace;
  the server revalidates the selected agent after reload or access changes.
- Open Chat explicitly. Merely viewing a profile never creates a conversation
  or starts a task. Existing conversations remain accessible while paused when
  their source access permits it.
- View saved progress without sending another model request. My requests is the
  default; All work I can view additionally includes source-visible requests by
  other people. Work shows active requests; Activity includes finished work.
- Stop one authorized request. This uses the existing exact cancellation path;
  it does not pause the whole agent or stop neighboring work. Cancellation is
  saved with actor and time. An already dispatched external effect cannot be
  undone by Stop; uncertain effects retain their separate review requirements.
- Open the original conversation and thread, including requests older than the
  latest history page, and review an exact approval through existing controls.
- Keep existing profile editing, guided permissions, approved knowledge and
  local-agent lifecycle controls.
- Save successful-outcome and routine guidance and review it in Overview.
  Maintainers can explicitly [try a personal draft-only sample](agent-job-rehearsal.md),
  review its selected context and reopen its saved result without rerunning it.

## Request API and authority

`GET /api/identity/workspaces/{workspace}/agent-profiles/{agent}/requests`
accepts `scope=mine|visible`, `status=active|all` and a positive `before` cursor.
It returns at most 25 candidate requests per page and a cursor when more exist.
Privacy filtering can make a page shorter. Progress reads do not claim jobs,
renew execution leases or invoke a provider. The exact saved record is also
available at `.../requests/{reply}`.

Both reads require current workspace membership and agent directory access.
Each request also requires current access to its source conversation and any
Library source. Being an Owner, agent maintainer, or selected approver does not
make a private conversation visible. Directory pending counts follow the same
source visibility instead of exposing the agent's hidden queue size.

Summaries come from the original human message, bounded to 180 characters;
the assembled model prompt is never returned as a summary. Saved progress is
bounded to 1,000 characters. The original requester's execution access is
checked independently: changed access hides live details while allowing an
otherwise authorized requester/Owner to cancel visible work. Permission to
view another person's work does not grant permission to stop it.

`GET /api/identity/workspaces/{workspace}/chats/{chat}/messages/{message}`
returns a bounded history page ending at that exact message and includes needed
thread roots. Source authorization, Library validation, history and reply
projection run under one workspace read transaction. The client aborts obsolete
focus reads and clears/revalidates on access changes.

Loaded request pages refresh together every five seconds while visible, with
explicit Refresh progress and Load more controls. Late responses cannot replace
a newly selected agent or request scope. Stop uses a synchronous submission
guard and retains a failed request for inspection and retry.

## Existing private local agents

Owners retain a separate Existing local agents roster. Those agents use their
existing private runtime, saved profile and access boundary. Work/Activity reads
the latest 100 private tasks from the selected agent's state API, and Stop uses
its existing task endpoint. Knowledge, access, automation, files and computer
tools are not silently converted into workspace business-operation grants.

## Deliberate limits

[Managed-agent schedules](shared-agent-schedules.md) now use the existing Automation
tab and controlled pooled background queue. Connected event waits,
browser/computer adapters and business email/phone are separate increments.
The relevant tabs state current support rather than showing fictional records.
The existing profile editor remains; this change does not implement every
creation screen in the design archive. No dependency or schema migration is
introduced by the Agents layout increment. It used schema 22; current
[questions and forms](agent-questions.md) introduced schema 23 separately, and
current full startup follows the [PostgreSQL setup requirement](postgres-workspaces.md).

## Verification

`cargo test --locked agent_request_tests -- --ignored --test-threads=1`, with a
disposable `RELAY_TEST_DATABASE_URL`, checks private work/count denial, visible
work versus Stop authority, revoked execution access, stable bounded pagination,
read-only progress, cross-workspace denial, old thread focus and Library revision
revocation. `web/tests/agent-workspace.test.mjs` checks explicit mutations,
duplicate Stop, failed retries, selection preservation, stale reads and private
runtime identity.

With the same disposable database and a built web client/binary,
`node --test tests/agents_team_workspace.mjs` checks the selected desktop layout,
actual channel/thread work, read/ask MCP permissions, free progress reads,
queued/running/waiting Stop, private work isolation, stale approval denial,
restart history and phone/dark layouts. It uses pinned Goose, a scripted local
model and an authenticated MCP fixture; this is local application evidence,
not commercial-model calibration or cloud acceptance. The earlier
`tests/agents_workspace.mjs` private Chat panel journey is preserved separately.

[Connected events](shared-agent-events.md) now share the Automation pane with schedules,
using reviewed source/read authority and the same ordinary task controls. Source
revocation clears affected readers without closing unrelated setup forms.

[Channel handoffs](channel-agent-handoffs.md) add source-authorized parent/helper
relationships to Work without granting new request or business authority.

Saved tasks also expose [Check progress in Chat](chat-task-progress.md) through the
original conversation, independently of permission to request new agent work.

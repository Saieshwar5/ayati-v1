# Agents team workspace

> **Updated 2026-11-21 (the one-page agent profile):** an agent's page has no
> tabs. The sections below and the 2026-10-31 note describe earlier layouts
> where they differ.
>
> **The list** has three groups: **Yours** (your assistant), **Team** (with its
> count) and, for admins, **Members' assistants**. Every face carries the same
> state dot as in Chat: green takes work now, purple is working, hollow is paused
> or has no owner. A row whose dot is not green says why ("2 working", "Paused",
> "Needs an owner"), in its text and its accessible name. The search box appears
> from nine agents on. The footer line is gone.
>
> **The header** is the face with its dot, the name and one line: "Ready · its
> job · owned by <faces> You", "Working · 2 requests · …", "Paused · …". The line
> has no dot of its own (2026-11-21); the face carries it.
> Someone who cannot resume a paused agent reads who can. An assistant is named
> by whose it is. **Message** (was Open in Chat) shows only when the server says
> the viewer may talk to it (`can_talk`). **Pause** and **Resume** are buttons
> for its owners and admins; ⋯ keeps Name, picture and role and Owners.
>
> **The page** (`web/src/react/agents/profile/`) is six rows of the real things,
> with Recent work beside them:
>
> | Row | Shows | Clicking a thing | ＋ |
> | --- | --- | --- | --- |
> | **Does** (an assistant: **Rules**) | the ground rules as text, then its skills | the text becomes a box in place (Save, Cancel, Esc); a skill opens its drawer, with Remove | the skills picker; **Ask <assistant>** opens the change-by-chat panel; **Cuckoo's rules** opens everything it receives |
> | **Knows** | Notes, Documents, Lessons (with "n to review"), About you (your own assistant) | its panel | Note or Document; **Conversation context** opens the preview |
> | **Can use** | each app with a state dot, MCP servers, computers, web search | one app alone with its rule per action, or that group | App (the catalog), Computer, MCP server, Web search |
> | **On its own** | each schedule and trigger as a line with its next run; Regular checks (your own assistant) | the automations panel | New automation |
> | **People** | owners and who can message it | the Owners or audience dialog | |
> | **Model** | the model | model, usage and budgets | |
>
> - A row that holds several kinds of thing asks which in a small menu; one kind
>   adds at once. While a row is empty its ＋ chips carry a word ("＋ App"), so a
>   new agent's page says what belongs where. Rules can start from a starter.
> - The right-hand column is **Recent work** until something is opened, then the
>   editor for that one thing; Close brings Recent work back and the rows are
>   read again. On narrow windows the page is one column and a panel takes the
>   whole page. The editors are the ones the tabs had (`ConnectionsTab` and its
>   `part`, `Knowledge`, `AlwaysRead`, `LearnedLessons`, `AutomationPane`,
>   `EnvironmentTab`, `AgentSkillChat`), with their long explanations removed.
> - The rules box keeps the revision it opened at, so a save never overwrites
>   someone else's newer change; a refused save keeps the box and its text. A
>   proposal from the assistant lands in the same box and is saved by the person.
> - **The agent page decides no work.** Recent work is links: a request that
>   waits for a person opens Activity → Needs me, every other one its
>   conversation. There is no Review, Approve or Stop here, and a waiting tool
>   action under Connections links to Activity too. Approvals are answered in
>   Activity or in the conversation.
> - People who cannot change the agent read the same page without ＋, the pencil
>   or Pause; what only its owners manage (lessons, schedules) is not read for
>   them, and Recent work is "Your requests".
> - Links keep their section names (`agentSectionUrl`, `panelFor`): Connections
>   opens everything it can use, or with an action that one app with the rule
>   marked; Automations and Settings open their panels; Overview and
>   Instructions are the page itself. Older names (tools, environment,
>   schedules) still work.
> - The rows are read together in one go (`use-agent-summary.ts`), each part on
>   its own so one refusal does not blank the page; again when the agent's
>   record changes, on access changes, when the window is looked at again and
>   every 30 seconds while visible.
>
> Checks: web `agent-profile.test.mjs` (rows from real data, the ＋ menu and its
> keyboard, one app alone in the panel, Close reading the rows again, section
> links, the read-only page), `agent-page`, `agent-workspace`, `agent-setup`
> (rules saved once at the opened revision, a failed draft kept, starters),
> `assistant-page`, `agent-automations-tab`, `assistant-agent-setup` and
> `skill-writer` (the chat opened from the Does row; proposed rules landing in
> the page's box). Looked at in a browser against the simulation controller.
> The browser scripts `context_preview`, `versioned_context` and
> `library_folders` were updated for the page but **not run** here; several
> other scripts already named tabs that no longer existed before this change
> (Schedules, Tools, Knowledge, Environment, Activity) and are unchanged.
>
> Not built: changing an existing agent from your ordinary chat with your
> assistant (it needs server work), and per-item panels for one schedule or one
> MCP server (they open their whole group).

> **Updated 2026-10-31 (D233, M2):** the agent page now follows
> `designs/agents-simplified/`. **＋ New agent** opens a five-step create flow (2026-11-08)
> (Name and picture, Connectors, Skills, People, Controls; see [agent-built skills](../designs/agent-built-skills/README.md)) beside the list. An agent page has a
> header with **Open in Chat** and a ⋯ menu (Pause or Resume, Name and picture,
> Transfer owner) for its owner and admins, and six tabs: **Overview** (about,
> your open requests, key facts with Change links), **Activity** (request
> history; since M4 it is [Activity](activity.md) History filtered to the agent), **Instructions** (edited in place, with the locked Cuckoo system prompt,
> what it knows and channel history), **Tools** (today's rules until M3),
> **Schedules** and **Environment** (model, and "Coming later" cards). The
> drawer editor, the Work, Access, Knowledge and Automation tabs, the "Not
> connected" placeholders and "Try a sample task" were removed. Sections below
> describe the earlier B workspace where they differ.

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
Library source. Being an admin, agent maintainer, or selected approver does not
make a private conversation visible. Directory pending counts follow the same
source visibility instead of exposing the agent's hidden queue size.

Summaries come from the original human message, bounded to 180 characters;
the assembled model prompt is never returned as a summary. Saved progress is
bounded to 1,000 characters. The original requester's execution access is
checked independently: changed access hides live details while allowing an
otherwise authorized requester/admin to cancel visible work. Permission to
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

Admins retain a separate Existing local agents roster. Those agents use their
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

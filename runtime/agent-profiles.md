# Agent ownership and setup

There is one agent type (D231, D233; schema 45). Creation and editing use
`/api/identity/workspaces/{workspace}/agent-profiles`.

## Who can talk to it, owners and managers

- **Everyone may create agents.** The per-person creator right and its
  `/control/creators` route were removed with `relay_agent_creators`.
- **Who can talk to it** (`audience`) is `everyone`, `selected` (the owner,
  admins and the people in `relay_chat_access`) or `only_me` (the owner alone).
  `relay_agent_can_talk(workspace, agent, person)` is the single definition.
  Saves may send `people` to replace the selected list; omitting it keeps it.
- **In a channel or group** the agent belongs to, anyone who can see that
  conversation may ask a shared agent there (`agent_access_in`). Only-me agents
  cannot join channels, and choosing Only me is refused while the agent is in one.
- **Seeing and changing:** the owner and admins see and change every agent,
  including only-me agents (admins cannot talk to someone else's only-me agent).
  Other members see the agents they can talk to.
- **Managers** (`relay_agent_managers`) start as the owner; saves may send
  `managers` (at least one workspace member). Approvals start using them in M3.
- **Owner transfer:** `POST …/agent-profiles/{id}/owner` with `{owner, revision}`.
  The new owner replaces the old one as a manager; the audit log records a
  readable sentence. An exact retry by the previous owner succeeds.
- **Removing a person** moves all their agents to the person taking over; only-me
  agents move paused. An agent they managed alone falls back to its owner.
- Narrowing who can talk, or pausing, stops queued and waiting work that the
  requester may no longer ask for.
- Upgrade tests run this code on older schemas, where `scope` (private/workspace)
  and per-person grants still decide.

## Job title, instructions and change records (schema 46)

- `role` is the **job title** shown under the name (at most 80 characters, may be
  empty). `settings.instructions` is the **one instructions text** (24,000 bytes)
  for character, job and rules.
- Schema 46 joined responsibility, custom instructions, "ask a person when",
  successful outcome and routine under `## ` headings, in the order the agent read
  them. A responsibility of 60 characters or fewer became the job title. Revisions
  were raised so stale editors reload.
- The agent receives its name, job title, the locked Cuckoo rules and its
  instructions. Guidance saved by an older client in the old fields still reaches it.
- `POST …/agent-profiles/prompt` with `{name, role, instructions}` returns exactly
  that text for a draft ("See everything it receives"); nothing is saved.
- Every save writes a readable audit sentence, such as "Arjun changed Atlas:
  instructions; who can talk (Selected people → Everyone)". It names what changed,
  never the instructions themselves.

## Implemented boundaries

- Profile writes use a revision and stable ID; exact retries of a completed save
  are accepted, stale edits are rejected.
- Names, job titles and instructions are passed into execution. Instructions are
  guidance, not a new authorization mechanism.
- Paused agents reject new requests and fail the ongoing job authorization check.
  Pending job counts are displayed as pending, not inferred provider availability.
- Preset and uploaded PNG/JPEG/WebP avatars are supported, with a 2 MiB upload
  limit and authenticated workspace/agent-scoped access. Historical avatars remain.
- Providers are the existing configured OpenAI and Fireworks integrations. Model
  identifiers are editable; supported identifiers depend on the selected provider.
  Credentials stay on the server. Configuration is not proof a provider is reachable.
- New managed agents receive [attributed context](conversation-context.md) from
  the current conversation: up to 60 messages and a 14,000-byte serialized history
  budget, preserving the latest bounded request even if it alone exceeds that
  budget. Current requester metadata is supplied separately. The since-joining
  option filters by the recorded joining sequence. They never receive another
  channel's transcript.
- Maintainers can add, correct and forget approved notes. Agent-wide notes are
  explicitly usable in every conversation with that agent; conversation notes
  require conversation access and agent participation. Source references must
  belong to the same conversation. Forgotten records remain for audit but are
  excluded from retrieval. Retrieval is capped at 20 notes and 16,000 bytes.
- The Knowledge tab explains conversation context separately from approved notes.
  New notes require an explicit conversation audience or a deliberate “Every
  conversation with this agent” choice; there is no preselected shared audience.
  Existing note audiences and source references stay fixed. Editing/forgetting
  sends only the write contract, excluding metadata returned by a read.
  Failed drafts remain available and repeated submission is gated.
- A [conversation context preview](conversation-context-preview.md) shows current
  eligible messages and notes without running an agent. It checks the viewer’s
  current agent/conversation access and is not an earlier task’s exact prompt.
- Forgetting suppresses the approved note in future retrieval. It does not erase
  facts already quoted in conversation messages, exports, model responses or
  backups, and does not cancel an already-running inference. Dependency-aware
  invalidation and deletion remain separate planned work.

## Explicit remaining work

This is not the complete proposed capabilities platform. Managed browser
enablement is still rejected server-side. Business tools now use
[workspace connections](workspace-connections.md) and
[server-enforced controls](shared-agent-control.md); their supported operations and
provider limitations are documented there. Controller-backed agents can now
search/read [older conversation history](conversation-context.md) under current
task access. Isolated browser provisioning and automatic scoped memory formation
remain unimplemented.

Adding tools requires execution-time authorization and durable action receipts:
recovered conversation jobs must not repeat an uncertain external mutation.
Tool descriptions and model output must never authorize an operation. A channel
reader's identity, the requester, the maintainer and the approving person are
separate roles. No personal browser session should be attached to a shared job
as a shortcut.

## Migration and validation

Migration `m20261021_000007_agent_profiles` adds ownership, settings, avatar and
knowledge tables without deleting records. Existing shared profiles keep their
workspace scope and acquire their current workspace owner as maintainer. Their
original creator is unknown, not inferred from the current owner. Existing
revision-zero profiles keep their old execution context until explicitly saved.
Back up the database before migration and reapply the runtime table/sequence
permissions using the existing PostgreSQL setup tools.

Use `tools/company-simulation.sh test` for disposable PostgreSQL schema tests,
`npm --prefix web test` for UI behavior, and the existing check/build commands.
These checks do not prove live provider responses or external tool execution.

The explicit-audience UI is covered by `web/tests/knowledge-audience.test.mjs`
and `tests/knowledge_audience.mjs`. The latter uses a disposable PostgreSQL schema,
actual browser/backend writes and real Goose with local scripted inference. It
checks an actual metadata-bearing record can be edited/forgotten, immutable scope,
non-maintainer denial, private versus shared model input, and exclusion of a
forgotten note from a new task. The fixture does not repeat the private note in a
saved assistant answer; it therefore does not claim derivative erasure. Desktop,
narrow and phone/dark forms plus reload are checked without extra inference.

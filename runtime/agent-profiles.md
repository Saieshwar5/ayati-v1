# Agent ownership and setup

The PostgreSQL workspace Agents page supports personal and workspace profiles.
Creation and editing use `/api/identity/workspaces/{workspace}/agent-profiles`.
The local-owner runtime remains visible in a separate section of the same page;
its existing conversations, files, Apps, browser and connector controls are retained.
No existing profile or history is copied into the new runtime automatically.

## Implemented boundaries

- Private profiles belong to the creating user. Other members, including the
  workspace Owner, cannot list, read, edit or grant access to them. They support
  one-person direct conversations and cannot be added to channels.
- Workspace profiles are created by the workspace Owner. The maintainer and
  workspace Owner can edit; explicit agent grants control other members' access.
- Visibility is immutable. Profile writes use a revision and stable ID; exact
  retries of a completed save are accepted, stale edits are rejected.
- Names, responsibilities, custom instructions and escalation guidance are passed
  into execution. Escalation text is guidance, not a new authorization mechanism.
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

## Explicit remaining work

This is not the complete proposed capabilities platform. Managed browser
enablement is still rejected server-side. Business tools now use
[workspace connections](workspace-connections.md) and
[server-enforced controls](shared-agent-control.md); their supported operations and
provider limitations are documented there. Controller-backed agents can now
search/read [older conversation history](conversation-context.md) under current
task access. Isolated browser provisioning and automatic scoped memory formation
remain unimplemented. Existing local agents retain their established capabilities and
approval boundaries.

Adding tools requires execution-time authorization and durable action receipts:
recovered conversation jobs must not repeat an uncertain external mutation.
Tool descriptions and model output must never authorize an operation. A channel
reader's identity, the requester, the maintainer and the approving person are
separate roles. No local-owner runtime or personal browser session should be
attached to a shared job as a shortcut.

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

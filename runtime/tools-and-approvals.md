# Agent tools and approvals (schema 47)

One model replaces per-conversation rules, per-person delegation limits,
designated approvers and the guided permission setup (D231 slices 3–4, D233 Tools,
built in M3).

## Who sets what

- **Admins** set the workspace rule for each action of each connected account in
  **Settings → Connections**: Allow, Ask first or Block, plus "Someone other than
  the requester must approve".
  - A missing rule is the safe default: reads Allow, writes Ask first.
  - Connectors only declare read and write effects today, so there is no separate
    "delete is blocked" default.
  - API: `POST /api/identity/workspaces/{w}/control/connector-rules` with
    `{connection, operation, mode, needs_other, revision}`.
- **An agent's owner (or an admin)** switches connectors on for the agent and may
  make any action stricter on its **Tools** tab, or in create step 4.
  - `POST …/agent-profiles/{id}/control/tools` with `{connection, enabled}`.
  - `POST …/agent-profiles/{id}/control/rules` with `{connection, operation, mode, revision}`.
  - A rule looser than the admin's is refused.
- Rules belong to the agent, not to a conversation. They apply wherever someone
  may ask the agent. `relay_control_effective_rules` holds the stricter of the
  two for every connector the agent uses.
- `GET /api/identity/workspaces/{w}/control/available` lists connectors and
  their workspace rules for any member, for create step 4.

## Revisions and revocation

- An action's revision is the greatest of three values: its switch, its
  workspace rule and its agent rule, all taken from `relay_control_revision`. It
  only moves forward, and a change to one action leaves the others untouched.
- Any change invalidates that action's pending approvals.
- The event, hand-off, Home and context revocation triggers watch
  `relay_connector_rules`, `relay_agent_connections` and `relay_control_rules`.
- Re-reviewing, reconnecting or disconnecting an account removes every switch and
  rule for it: new operation meanings start from the safe defaults.

## Who decides "Ask first"

- **Any current manager** of the agent who can see the request may decide it, in
  the conversation, from Home attention or from the Tools tab. The first decision
  wins; a repeat by the same person returns it, and the opposite decision conflicts.
- **The requester** can't approve when the action needs someone else.
- **An admin** may decide after `relay_workspaces.approval_backup_hours` (4 by
  default; its setting screen comes with M5), or straight away when no manager
  can decide.
- **Who decided** is stored in `decided_by`, and their authority is checked again
  just before dispatch.
- **Removing a manager** leaves their requests with the other managers.
- **Changing a conversation's members while work waits** still fences that work
  (versioned context).

## Retired

- The guided setup: its routes, drafts and suggestion tasks. Saved suggestion
  tasks finish as cancelled. `relay_control_drafts` remains until a later cleanup.
- `/control/delegations`, `/control/approvers`, `relay_control_delegations` and
  `relay_control_approvers`.
- Approval channels: requests appear in the conversation where they were asked.

Earlier contracts ([shared control](shared-agent-control.md),
[channel approvals](channel-approvals.md), [guided permissions](guided-permissions.md),
[workspace connections](workspace-connections.md)) describe the replaced model where
they differ.

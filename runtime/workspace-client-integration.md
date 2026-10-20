# Authenticated Relay client

`ayati identity` serves the original Home panels, Apps, private agent conversations
and Settings behind `/w/<workspace-id>/`. The black Relay branding and shared
navigation now expose Home, Apps, Chat, Agents, Users and Settings. Sign-in and
workspace selection open this client, not a replacement dashboard.

## Existing installation and isolated workspaces

New workspaces have independent stores under
`AYATI_DATA_DIR/identity/workspaces/<id>`. They do not inherit the original owner's
VM, browser, files, tools or connectors. To attach an existing installation to its
owner's workspace, stop both server modes and run:

```
ayati adopt-workspace <workspace-id>
ayati identity
```

Adoption requires a single owner, an empty destination business store, no running
or queued work, no active schedules and exclusive daemon/VM locks. It makes a
private full installation backup with SQLite integrity checks before recording a
singleton binding. Data and absolute VM paths stay in place. That owner's private
client keeps the original agents, history, Apps, panels and execution settings.
Only the bound owner receives this legacy authority. Other workspaces remain
separate. `ayati serve` refuses to bypass the binding. With servers stopped,
`ayati release-workspace <workspace-id>` removes the binding without deleting data.
Backups retain credentials/private data and must never be committed or shared.

Workspace switching performs a full document navigation. Drafts, uploads,
controller tokens, receipts and Home layouts are scoped by user and workspace.
Only the adopted owner's old Home layout is copied into its new scoped key.

## Chat, Agents and Users

Chat keeps the original owner-only agent conversations accessible as private
chats. New conversations can include workspace people and explicitly permitted
agents. They have persistent membership and messages, idempotent sends and local
per-conversation drafts. Sending to everyone does not wake an agent; selecting
an agent explicitly asks it to reply.

Shared replies use the existing Goose runtime in a fresh, conversation-only
context. They receive only bounded shared transcript history, never private
agent history, memory, files, tools or the owner's VM. Agent access is checked
before dispatch, during execution and before publication. Revocation cancels
publication; removal clears conversation membership and grants. Pending replies
are marked failed after restart rather than replayed. Agent requests are bounded
and timed out. Tool-enabled shared work, editable group membership, unread counts,
notifications, attachments and threads are not implemented yet.

Agents provides creation, configuration, retry, archive, restore and removal.
The built-in guide cannot be removed. Archive stops its runtime. Removal is a
retained tombstone; files, conversations and App data are not erased. The existing
local four-agent registry limit still applies, including retained agents. Agent
creation requires the existing VM setup. Only owners manage agent configuration.

Users retains invitation and role management and adds explicit agent-chat, App
and panel access. Owners have implicit management authority. Other people receive
only granted published Apps and active panels. App access can be read-only or
read/write/delete records; neither permits code or schema changes. App document
and record sessions are bound to the authenticated member, workspace and App.
Private agent conversations, files, approvals and connectors remain owner-only.

## Request boundary

Every business request uses `/w/<id>/api/...` and validates the authenticated
session and current membership. A workspace or agent ID is never authorization.
Mutations require same-origin Origin and `X-Ayati-Request: identity`. Resource
grants are checked on each request; revocation and member record operations use
a shared lock. The inner business router receives fresh route parameters so
workspace wildcard parameters cannot break App or agent handlers.

SSE streams recheck local session/membership at most one second apart and provider
status every 15 seconds. Logout and access loss close streams. Unscoped business
APIs and host shutdown remain unavailable. Legacy connectors are available only
to the explicitly bound owner; fresh workspaces need provider-specific setup.

## Validation and limits

Rust fixture checks cover adoption/backup reopening, independent storage,
cross-workspace denial, shared conversation membership and deduplication,
revocation, restart recovery, published-only App access, member-bound App sessions,
read/write restrictions and retained agent removal. Existing App, Home panel,
profile, registry and durable-work regression suites also pass. Client unit tests,
production build and browser-test type checking pass.

This remains a local pilot. Automated fixtures do not prove VM provisioning,
backup VM boot recovery or production tenancy. See `identity-workspaces.md` for
hosted deployment requirements. Preserve the distinction between working private
agent tools and conversation-only shared agent replies.

Live local verification on 2026-10-20: the bound owner opened the original Diet
Tracker App and ramesh's saved private conversation. A new owner-only Chat received
a real Goose reply and retained the exchange after reload. Users showed both
existing memberships and the new access controls. No employee permissions were
changed for verification, and no new invitations were sent. The final library
suite passed 44 tests, with 23 focused existing feature tests and six client test
files also passing. Desktop and narrow Chat layout checks were performed.

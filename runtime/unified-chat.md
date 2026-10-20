# Unified workspace Chat

Workspace Chat uses a persistent React directory for Channels and Direct messages.
Switching conversations changes only the conversation surface. The workspace rail,
search field and conversation list remain mounted. Existing owner-only agent chats
keep their history, streaming, approvals, attachments and tools inside this shell.
Local installations without a workspace retain their private-agent view.

The quiet workspace rail sits outside the shared rounded workspace frame. Chat's
directory and conversation use the same surface color (white in light mode),
separated by a thin divider. Selection and hover highlights remain grey.
The pencil action starts a New message;
the plus beside Channels creates a channel. Both have accessible labels and
hover titles. On narrow screens, the list and conversation alternate as before.

New channels are private and require the creator plus at least one other person or
permitted agent. Existing named shared conversations remain visible as channels;
their membership and history are unchanged. New message lists workspace people and
agents available to the requester. Human DMs and granted member-to-agent DMs use a
canonical participant key, so either person reopens the same conversation, including
under concurrent creation. Owner agent selection retains the existing private runtime.
Channel membership is fixed at creation in this release; adding/removing channel
members and public channels are future work. Removing a workspace user still revokes
conversation access and grants through the existing membership system.

Messages have optional thread roots validated inside their own conversation. Channel
agent requests publish into the request thread. Ordinary messages do not run agents;
choose an explicit agent in the composer. Shared agent DMs reply in the main timeline.
Thread context contains only the root and replies; direct-message context stays in
that DM. These shared executions have no private agent memory or business tools.
Typed @names alone do not dispatch work. Existing requester grants, membership checks,
durable jobs and result-publication checks continue to apply.

The current history response contains the latest 200 messages plus any older roots
referenced by those messages. It is a recent-history window, not full pagination or
an exhaustive thread reply count. Agent context remains capped at 30 messages and
14,000 characters. Unread counts, push notifications, typing indicators, attachments
in shared conversations, channel lifecycle controls and older-history pagination are
not implemented here. Polling remains 2.5 seconds for the open conversation and five
seconds for the directory, paused when the document is hidden.

## Storage upgrade

PostgreSQL migration `m20261020_000002_conversation_kinds` adds conversation kind,
purpose, unique direct participant keys and message thread roots. Run
`./tools/local-postgres.sh migrate` locally (or `workspace-db migrate` with the
administrator connection), then restart Relay. Startup requires both migrations.
The migration is additive and retains all existing messages and memberships.
SQLite fallback upgrades its local schema on open. Offline SQLite import supports
both the original schema and the new conversation metadata and thread columns.

## Validation

Client regression checks cover persistent navigation across private agents, channels
and DMs, isolated drafts, stale-response rejection, channel member selection and
thread submission. SQLite and real PostgreSQL tests cover membership, duplicate
messages, concurrent DM creation, foreign thread rejection, thread context isolation,
revocation and import. Live-provider agent behavior remains separately testable; a
passing storage test does not claim a new model-provider integration run.

The chat surface uses compact headers and single-row composers. The @ control
opens recipient selection; typing an @name still does not dispatch an agent.
Shared-chat scope information lives in Details. The always-visible Create panel
shortcut above private-agent composers is removed; Home's creation flow and
explicit creation-mode links remain available.

Agent tools remain available from the compact header overflow menu.

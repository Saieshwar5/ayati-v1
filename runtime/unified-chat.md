# Unified workspace Chat

PostgreSQL-managed agent requests can run in an
[independent worker](independent-chat-worker.md). Set `AYATI_CHAT_EXECUTION=external`
on the API and start `ayati worker` separately. Embedded execution remains the
default; both paths share conversation validation, source grants and reply claims.

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
On PostgreSQL, channel creators/managers can update people membership, and managers
can rename channels, switch private/public visibility and restrict posting to managers.
Public visibility includes existing history and published files. SQLite retains fixed
private membership. Removing a workspace user still revokes
conversation access and grants through the existing membership system.

Messages have optional thread roots validated inside their own conversation. Channel
agent requests publish in the main channel timeline unless the request explicitly
belongs to a thread. Ordinary messages do not run agents; select an inline @mention
to request an agent. Shared agent DMs reply in the main timeline.
Thread context contains only the root and replies; direct-message context stays in
that DM. These shared executions have no private agent memory or business tools.
Typed @names alone do not dispatch work. Existing requester grants, membership checks,
durable jobs and result-publication checks continue to apply.

The history response contains a bounded 200-message page plus older roots referenced
by that page. PostgreSQL exposes `next_before` for older pages and the client merges
loaded history by message ID. SQLite retains its recent-history window. Thread reply
counts reflect loaded messages, not exhaustive totals. Agent context remains capped at 30 messages and
14,000 characters. PostgreSQL supports scoped shared attachments, image/audio display and voice recording,
with immutable local objects and download-time access checks. Unread counts and live delivery now follow [reliable messaging](reliable-messaging.md). Push
notifications and typing indicators remain unimplemented.
[Personal pins and guarded channel archive/restore](chat-organization.md) are implemented separately; accepted work retains its original authority. Legacy baseline polling was 2.5 seconds for the open conversation and five
seconds for the directory, paused when the document is hidden.

## Storage upgrade

PostgreSQL migration `m20261020_000002_conversation_kinds` adds conversation kind,
purpose, unique direct participant keys and message thread roots. Run
`./tools/local-postgres.sh migrate` locally (or `workspace-db migrate` with the
administrator connection), then restart Relay. Startup requires all six current migrations, including company
collaboration and queue telemetry. See [company simulation](company-simulation.md).
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

The chat surface uses compact headers and a shared expanding React composer. Typing
@ opens inline suggestions; selecting an agent records its identity for dispatch.
Unselected plain @text does not dispatch work. The separate recipient selector is removed.
Shared-chat scope information lives in Details. The always-visible Create panel
shortcut above private-agent composers is removed; Home's creation flow and
explicit creation-mode links remain available.

Agent tools remain available from the compact header overflow menu.

## Company rehearsal additions — 2026-10-21

PostgreSQL shared profiles separate agent identity from private VM provisioning.
They retain explicit employee request grants. Four local workers draw from a bounded
64-pending queue, waking on accepted work/completion with a recovery-poll fallback.
Normal Goose shared execution remains conversation-only. Simulation-only scripted
execution can operate on internal quotation records behind explicit channel grants,
manager approval, claim fencing and atomic receipts. It never calls model providers.
Teams, channel-scoped feedback, local object storage, retained scenario reports and
measured limitations are described in [company simulation](company-simulation.md).

## Chat experience follow-up

See [unified composition and Markdown](chat-experience.md) for inline identity-bound
mentions, safe formatted responses, integrated file/voice controls and channel-visible
agent replies. Explicit threads retain their thread placement; old records are preserved.

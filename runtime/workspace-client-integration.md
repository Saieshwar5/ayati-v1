# Authenticated workspace client

`ayati identity` serves the web client behind `/w/<workspace-id>/`. Sign-in and
workspace selection open this client. The account menu holds workspace
switching, email, appearance and sign-out. Switching workspaces performs a full document
navigation. Drafts, uploads, controller tokens, receipts and Home layouts are
scoped by user and workspace.

## One backend shape

Locally and in the cloud the controller serves the same shared product from
PostgreSQL: Chat, shared agents, approvals, questions, Home attention and panels,
Library, schedules and events. Agent work runs on executors (see
[executor pool](executor-pool.md)).

There is no per-workspace private runtime. The single-user daemon's private
agents, local Apps and panels, memory, browser, VM and connector screens were
removed on 2026-10-29. Their shared replacements are built feature by feature
(D221 Step 9) rather than proxied to old local stores.

## Request boundary

Every business request uses `/w/<id>/api/...` and validates the authenticated
session and current membership. A workspace or agent ID is never authorization.
Mutations require same-origin Origin and `X-Ayati-Request: identity`.

`/w/<id>/api/context` returns the user, workspace and role. Older workspace API
paths with no shared implementation behave exactly as in hosted mode:

- `GET` of `home`, `home/preferences`, `apps`, `panels` and `agents` returns an
  empty view (`apps`/`panels` report `"available": false`).
- Every other legacy path returns `503` with a "not available yet" error.

The company simulation additionally lists its scripted shared agents at
`/api/agents`. Unscoped business APIs and host shutdown are unavailable.

## Validation

Rust gateway tests cover hosted-equivalent empty views and `503` answers, session
and same-origin requirements, and membership loss. PostgreSQL regressions cover
cross-workspace denial, shared conversation membership, revocation and restart
recovery.

The [React migration](react-client.md) owns the shell, account/workspace flows,
Users and the shared screens.

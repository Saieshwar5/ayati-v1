# Editing agent names and responsibilities

Open Agents → Details → Edit name and responsibilities. The owner can edit any
ready Goose agent, including the original local agent. Empty responsibilities
mean a general assistant. Names must contain text. Byte limits are 80 for names
and 4000 for responsibilities; control characters other than newline are rejected.
Model, tools, credentials and VM settings are not editable in this increment.

`POST /api/agents/{id}/profile` accepts only `name`, `role`, and
`expected_revision`. `/api/agents` exposes each profile's current `revision` and
`editable` flag. The endpoint is a whole-workspace local-owner operation, like
agent creation; the path identifies the target. Selection is not authentication.
Existing same-origin/loopback checks apply. Unknown/not-ready agents fail closed.

Edits require an idle lifecycle gate, no queued/running tasks, no pending
approvals, and no owner browser takeover. The UI does not implicitly cancel
work. Finish it or use existing Stop controls and wait until the agent is idle.
The database transaction checks accepted work; requests arriving after commit
use the new settings. Future schedules retain their definitions and use the new
responsibilities, as disclosed in the editor.

Schema 7 introduces a per-agent singleton `profile_edit` record. Name, role,
revision, role-generation and removal of an obsolete runtime-session mapping
are committed together. The registry's original creation receipt remains
unchanged, so retrying agent creation still compares the original request.
Profile overrides are read from each agent's private database for display and
runtime use. Older binaries reject schema 7 instead of ignoring edited roles.

A name-only edit preserves model context. A changed role increments its generation
and removes the current session mapping; the next request drops any cached
process with the old generation and creates a new Goose session. Visible chat,
files, durable memory and task outcomes remain intact. Old conversation turns
are not replayed, and no request is automatically resubmitted. The original Goose
session files are retained; the change does not erase them. Restoring an old
role still creates fresh context. The role is an instruction, not a permission:
existing code continues to enforce the same tool and approval boundaries.

Stale revisions are rejected rather than overwriting another tab's changes.
There is no automatic retry after a lost response. Reopen the editor after its
profile refresh to inspect the saved state before submitting again. A daemon
restart restores the committed edit or the preceding state, never a partially
updated role/session pair.

Checks: `cargo test --test profile_storage --test runtime_storage` and
`node --test tests/agent_profile.mjs`. The browser scenario uses real pinned
Goose and a synthetic provider; it verifies name continuity, role/context
replacement, preserved history, busy/stale rejection and restart. The two-agent
KVM scenario in `tests/independent_agents.mjs` verifies an edited second role
and memory/history isolation across restart. No paid inference is required.

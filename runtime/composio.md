# Composio business connections

Ayati uses Composio's v3.1 HTTP API through the existing reqwest client. Composio
owns provider sign-in and OAuth token refresh. Ayati owns account metadata,
per-agent operation grants, exact approvals, run fencing and execution outcomes.
No connector SDK or new agent harness is included.

## Setup and use

Set `COMPOSIO_API_KEY` in `.env`, restart Ayati, and open Settings → Business
connections. Create Google service auth configurations in the same Composio
project first. The initial UI supports Gmail, Google Drive and Calendar; this
does not claim tested access to the entire Composio catalog.

Choose Connect Google account, select a configuration, label the account, and
open the private sign-in link. Complete sign-in yourself, then Refresh account.
The host stores account identifiers, not provider tokens. Do not share the link.
Under the selected agent's Details → Assigned connections, search for operations
and grant individual actions. Grants pin the returned version and schema.
One agent's grants do not authorize another. At most 16 operations are assigned
per agent; schemas and results are bounded. Composio validates tool arguments.

Goose receives `ayati_workspace__connector_tools` and `connector_execute` through
stdio MCP. The host broker resolves the agent identity, consumes one exact owner
approval, checks the current grant, verifies Composio's account user/toolkit/status
and checks cancellation before execution. The model cannot choose a provider user,
override credentials or select a version. No general proxy tool is exposed.
The Composio project key and provider tokens never enter the VM.

Revoking workspace access affects every local grantee and persists even when
Composio is unavailable. Removing an operation affects only the selected agent.
Execution and revocation share a gate: after revocation returns, a waiting or new
call cannot use the old grant. An already-dispatched action may finish; Stop
cannot undo it. Workspace revocation does not revoke Google consent or delete
the remote account; use Google/Composio controls for that.

## Recovery and compatibility

Adding workspace tools starts fresh Goose working context once, only when all
other saved settings match. Conversation, files and memory remain. Other settings
mismatches still fail closed. Guest installations need the updated `ayati-guest`
binary with `workspace-mcp`. Old guests never fall back to host execution. Use a
separately prepared guest until an environment-upgrade path is validated.

Schema 10 adds consumed-approval receipts without deleting saved state. Back up
important state before upgrading; older binaries reject this database.
External execution is recorded before dispatch. Lost replies, malformed responses,
provider errors and process loss retain uncertain records. No action is retried
automatically. Inspect the actual account before deliberately repeating an action.
`/api/connector-executions` returns the selected agent's latest 50 records.
Unacknowledged sign-in link creation remains `LINK_UNCERTAIN`; inspect Composio
before starting another flow. It is not automatically retried.

## Verification

`tests/composio_integration.mjs` uses real pinned Goose/ACP/MCP with a synthetic
Composio server. It covers approved execution, exact account/user/version,
revocation while waiting, foreign/expired accounts, lost replies and restart.
`tests/workspace_authority.rs` checks exact single-use approvals, cancellation,
endpoint restrictions and the compatible context transition.

Live metadata checks on 2026-10-17 returned HTTP 401 with the keys configured
during development. No Google mailbox, files or calendar was accessed. Live
linking, expiry/refresh, scopes and event delivery remain acceptance gates.

References: [account lifecycle](https://docs.composio.dev/docs/auth-configuration/connected-accounts),
[Connect Links](https://docs.composio.dev/reference/api-reference/connected-accounts/postConnectedAccountsLink),
[schemas](https://docs.composio.dev/reference/api-reference/tools/getToolsByToolSlug),
[execution](https://docs.composio.dev/reference/api-reference/tools/postToolsExecuteByToolSlug).

Isolated tests may set `AYATI_COMPOSIO_BASE_URL` to HTTP on 127.0.0.1. Other remote
hosts and redirects are rejected. Never use real keys with a fixture. Member
authentication remains deferred; keep the owner daemon on loopback.

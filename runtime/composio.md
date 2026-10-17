# Composio business connections

Ayati supports Composio Connect MCP with the owner's consumer key and the
Platform API with a project key. Composio owns provider sign-in and OAuth token
refresh. Ayati owns local account selection, per-agent operation grants, exact
approvals, run fencing and uncertain execution outcomes. Provider credentials
remain on the host; no general Composio executor is exposed to agents.

## Composio Connect MCP

Keep the key from **Sessions & API Key** in `COMPOSIO_API_KEY` in local `.env`.
A `ck_` key selects `https://connect.composio.dev/mcp` and the
`x-consumer-api-key` header automatically. Restart Ayati after changing the key.
No developer-project key or Google API client configuration is needed for this
path. Connect MCP and Platform accounts belong to separate Composio projects.

In Settings → Business connections → Connect an account, discover a service,
select an existing active Composio account or create a new sign-in link, and give
it a local label. Complete any provider consent yourself, then Refresh account.
In an agent's Details → Assigned connections, discover and grant exact operations.
Importing an account alone grants no agent access. Existing consumer connections
are verified through MCP; account IDs are always passed explicitly on execution.
The UI strips provider identity details and authentication state from metadata.

The host uses MCP initialization, `COMPOSIO_SEARCH_TOOLS`,
`COMPOSIO_GET_TOOL_SCHEMAS`, owner-only `COMPOSIO_MANAGE_CONNECTIONS`, and one
assigned operation at a time through `COMPOSIO_MULTI_EXECUTE_TOOL`. Remote Bash,
Workbench, generic executors and connection management are not model-facing tools.
Transport supports JSON and streamed SSE replies, including comments/chunking,
with bounded responses, timeouts, no redirects and no automatic operation retry.

Consumer MCP does not expose provider version pins. A grant instead records a
SHA-256 fingerprint of the complete operation definition; the host fetches it
again and blocks execution if it changed. This detects schema drift, not upstream
implementation changes with the same schema. This catalog does not expose
Platform event-subscription management: the trigger editor shows that limitation.
Schedules can use assigned account operations with the usual approval policy.

## Platform API compatibility

A Platform project key uses the v3.1 HTTP API and `x-api-key`. Create service auth
configurations in that developer project, select one in Settings, complete the
private sign-in link, and refresh the account. Platform grants pin explicit
provider toolkit versions. A connection from the other authentication mode is
rejected rather than silently reused. The integration can discover other
owner-configured toolkits; Gmail is the live consumer acceptance target so far.

At most 16 operations are assigned per agent; schemas, arguments and results are
bounded. Composio validates tool arguments. One agent's grants never authorize
another agent.

Goose receives `ayati_workspace__connector_tools` and `connector_execute` through
stdio MCP. The host broker resolves the agent identity, consumes one exact owner
approval, checks the current grant, verifies Composio's account user/toolkit/status
and checks cancellation before execution. The model cannot choose a provider user,
override credentials or select a version. No general proxy tool is exposed.
The Composio key and provider tokens never enter the VM.

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

Live checks on 2026-10-17 established that the owner's consumer key authenticates
with HTTP 200 at the Connect MCP endpoint. The Rust client passed live discovery,
full-schema retrieval, schema-fingerprint matching and sanitized account listing.
A separate bounded read-only wire check executed `GMAIL_FETCH_EMAILS` for one
message ID with payloads disabled; no message body was requested or data displayed.
The earlier HTTP 401 results came from sending this consumer key to Platform APIs;
they were not evidence that the key itself was invalid.

`tests/composio_mcp.mjs` exercises real Goose and browser UI against a synthetic
MCP server: exact account selection, grants/approval, unauthorized tools, schema
drift, expiry, revocation, lost replies and restart. An explicit metadata-only
live Rust check is available with
`cargo test --locked --test composio_mcp_live -- --ignored`.
No live new-account OAuth consent, token expiry/refresh, VM acceptance or external
event delivery is claimed. The new-account link response path is fixture-tested.

[Signed event triggers](composio-triggers.md) now reuse the same account boundary.

References: [account lifecycle](https://docs.composio.dev/docs/auth-configuration/connected-accounts),
[Connect Links](https://docs.composio.dev/reference/api-reference/connected-accounts/postConnectedAccountsLink),
[schemas](https://docs.composio.dev/reference/api-reference/tools/getToolsByToolSlug),
[execution](https://docs.composio.dev/reference/api-reference/tools/postToolsExecuteByToolSlug).

Isolated tests may set `AYATI_COMPOSIO_BASE_URL` to HTTP on 127.0.0.1. Other remote
hosts and redirects are rejected. Never use real keys with a fixture. Member
authentication remains deferred; keep the owner daemon on loopback.

Consumer references: [Composio Connect](https://docs.composio.dev/docs/composio-connect)
and [consumer/project authentication boundaries](https://docs.composio.dev/kb/guide/consumer-project-boundaries-and-auth-selection).
The consumer server identifies itself as version 0.1.0; the integration negotiates
MCP 2024-11-05. It reuses reqwest 0.13 and ring 0.17.14 already in the lockfile;
no Composio SDK or runtime dependency is required.

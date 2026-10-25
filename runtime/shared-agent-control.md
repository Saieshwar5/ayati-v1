# Shared agent tools and exact approvals

The first shared-workspace control slice adds reviewed remote MCP connections,
per-agent/per-conversation Allow automatically / Ask first / Block rules, exact
action approvals and durable outcomes. PostgreSQL schema **20** is required by the current checkout.
[Workspace connections and delegation](workspace-connections.md) extends this engine. [Guided reviews](guided-permissions.md) add draft/preview/activation.
The feature is disabled by default: absent operator profiles or assigned rules,
managed agents retain the existing tool-free execution path.

## Owner journey

Open **Settings → Workspace → Connections** to approve configured systems, verify
accounts and delegate bounded operation access. Then open Agents → **Tools and
approvals**. Activity shows exact approvals/results; Permissions saves rules for
one selected conversation per operation. Each operation starts blocked. Owners
configure rules directly; delegated members configure agents they maintain within
their operation limit. Ask first names the saving Owner for direct Owner rules,
or the Owner who granted the member's delegation. Other Owners cannot substitute
for that designated approver.

Connection setup validates reviewed remote schemas without executing a business
operation. Rule changes invalidate pending proposals. Reconnecting/reviewing or
disconnecting also blocks existing rules and delegation limits. Old permissions
never silently authorize changed credentials or operation meanings.

Agent requests use `ayati_control` forwarding tools. The controller validates the
current requester, workspace, agent, conversation, Library source access, running claim, connection, rule,
input schema and exact action. Approved proposals remain inspectable after the
Goose job ends; an authorized human's later decision owns a separate bounded action
dispatch. This does not implement automatic continuation of the original Goose job.

The drawer displays pending approval, executing, completed, blocked, rejected,
invalidated and uncertain states, with exact inputs and recorded results. It
supports desktop/phone browsers. Existing local-owner tools, private browser,
memory and default Goose extensions are not enabled for shared agents.

## Operator setup and supported boundary

Configure `AYATI_MCP_SERVERS_FILE` **only on the controller**, using an absolute,
owned regular 0600 JSON file (maximum 64 KiB). Each profile explicitly binds one
workspace UUID, endpoint and independently reviewed operation meanings/schemas.
See [the example](../integrations/remote_mcp/servers.example.json). Put any bearer
credential in a separate absolute owned 0600 file; the profile contains its path.
Neither the profile nor credential belongs in Git or ordinary agent conversation.

```sh
# Paths are private operator inputs, not an instruction to create credentials.
AYATI_MCP_SERVERS_FILE=/absolute/private/servers.json ayati identity
```

The controller manages authentication; the workspace owner chooses among that
workspace's configured systems. This slice does not provide self-service arbitrary
server URLs, OAuth registration or credential entry/storage in the browser. An
operator must establish that the server and its credentials restrict access to
the intended account/data. Discovery/annotations alone do not prove effects.

Supported transport: Streamable HTTP MCP 2025-11-25, JSON or bounded SSE replies,
one non-paginated catalog up to 200 tools, up to 20 reviewed operations/profile.
HTTPS endpoints use pinned public IPv4 DNS answers, no redirects or proxy, and
bounded requests. Explicit operator-configured `http://127.0.0.1` is supported for
local verification. IPv6/private-network endpoints and other transports are not
accepted yet. Each remote session retains one credential snapshot; credential,
endpoint or schema changes require connection review and invalidate old use.

Arguments use a closed, flat object schema: explicit properties/required fields,
string/integer/number/boolean fields and optional enum. Other JSON Schema keywords
are rejected rather than silently ignored. Limits are 20 fields, 8 KiB encoded
arguments, 2,000 bytes/string, browser-safe JSON numbers, 64 KiB remote replies and
20 action identities/job.
Rules cover the account's available records in **one selected conversation**.
Additional per-record, amount, aggregate-business-budget or time predicates are
not available. No rule is inferred from a role prompt or tool description.

## Exact requests, dispatch and uncertainty

The controller derives a request identity from normalized connection, operation
and arguments within the durable job. Repeating those exact fields, including
after worker recovery, returns the saved action and never dispatches it twice.
An intentional repeat needs a new task. This is scoped request deduplication,
not a claim of universal exactly-once external effects.

Database transactions serialize rule changes, approval decisions and dispatch
claims per workspace. No DB transaction is held across remote requests. The
controller rechecks current authority after remote setup/schema validation and
immediately before the operation call. Revocation cannot undo an already dispatched
effect. Approvals bind immutable inputs and rule/connection revisions; opposite
decisions conflict and repeated decisions return the existing outcome.

Failures before sending `tools/call` invalidate the request. After dispatch, a
timeout, lost response, malformed reply or remote error remains uncertain. New
writes on that connection are held while another write is executing or uncertain;
reads may continue. A crash can leave an executing record; after 60 seconds the
read projection labels it uncertain without granting another dispatch. No replica
automatically retries an action or resets another replica's live state.

The first slice has no provider-specific automatic outcome lookup or operator
reconciliation UI. Verify the external effect and implement/use a reviewed
reconciliation procedure before unblocking uncertain writes. Do not delete the
receipt or fabricate success. Full multi-day Goose continuation, automatic memory,
cloud computer/browser/App operations and the commercial usage ledger remain
separate launch work.

## Executor boundary and performance

Embedded controller execution binds a fresh loopback broker to the exact job.
Controlled workers without database/WorkOS/MCP credentials use the same authenticated assignment,
session and current lease as the existing machine API. `/v1/assignments/{id}/tools`
is the only added machine route. Its scope is validated before tool admission.
Each broker has a new local capability, no browser cookies and no provisioning or
remote MCP credential. It closes when the owned attempt ends. Legacy direct-PG
polling/assignment workers remain conversation-only; enabling new tool grants is
gated by controller-managed execution, and local database worker code fails closed
if given a tool-enabled job. Do not distribute MCP credentials to those workers.

Prepared slots remain tool-free. For an explicitly tool-enabled job, the executor
stops the unused prepared process, discards its empty session pointer and starts a
fresh tool-configured Goose harness. It never loads a previous tenant transcript
with new tools. Output retains the existing ordered durable stream. This path has
a **cold tool-harness startup** and does not inherit earlier prepared-chat latency
claims. The next slot is cleanly prepared after completion.

## Verification

The source checks are:

```sh
cargo test --locked --lib
# Dedicated disposable local PostgreSQL only:
cargo test --locked --lib identity::postgres -- --ignored --skip embedded_gateway_uses_controller_owned_credentials
node --test tests/shared_agent_control.mjs tests/workspace_connections.mjs tests/guided_permissions.mjs
cd web
npm test
npm run build
npm run check:e2e
```

The process journey uses the real pinned Goose, a scripted local model, an actual
authenticated local HTTP MCP server with independent sample records, PostgreSQL,
and Playwright. It is local protocol/product evidence, not a live commercial MCP
provider, live model quality, cloud isolation, billing or production acceptance.
Use `RELAY_TEST_DATABASE_URL` for a dedicated `relay_simulation_*` database. Test
records, failed attempts and screenshots are retained beneath `.tmp/`.

Historical schema-18 verification on 2026-10-25: 82 Rust unit tests, 94 disposable-PostgreSQL tests,
80 client behavior tests and 43 AWS manifest/bridge tests pass. Clippy with warnings
denied, formatting, source line limits, client production build and browser-test
type checks pass. The two process tests cover the pooled real-Goose/browser flow,
the embedded controller-local gateway (including invalid capability denial), and
legacy direct-DB execution gating. The embedded gateway test uses the same HTTP
fixture and is invoked by the process test; it is deliberately excluded from the
standalone PostgreSQL command above. It does not prove an embedded Goose end-to-end
journey. Separate real-Goose regressions cover prepared two-agent concurrency and
the database-free controlled worker, controller replacement, renewal and Stop.

Retained evidence: `.tmp/shared-control-verification/`, browser journey
`.tmp/independent-worker-BdzsGP`, local gateway `.tmp/embedded-control-cxtyFd`,
and prepared-pool regression `.tmp/independent-worker-8Ufr8F`. The browser evidence
includes pending approval at desktop/phone sizes and completed/uncertain outcomes
in light/dark themes. No paid inference, ordinary workspace migration or cloud
provisioning was performed. The existing AWS development instance remained stopped.

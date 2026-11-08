# Connector layer: next work (proposal)

Written 2026-11-08. Nothing here is built or approved. It builds on
[connector providers](../../runtime/connector-providers.md) (schema 53) and
[agent tools and approvals](../../runtime/tools-and-approvals.md) (schema 47).
Pricing and API facts were read from vendor pages and third-party summaries on
2026-11-08; each provider plan starts with a small real integration check that
records the tested version, as `AGENTS.md` asks.

## Where we are

Working and tested against local fakes:

- One `ConnectorProvider` contract in `src/identity/connectors/`, with a
  conformance suite. Two providers pass it: operator MCP servers and Composio.
- Cuckoo owns the catalog (`catalog.json`), approved apps, accounts, rules,
  approvals, receipts and Activity. Agents see only `operations` and
  `request_action`.
- A saved connection already stores its provider, and dispatch already uses it
  (`control::dispatch` calls `connectors::named(&dispatch.provider)`). A
  connected account is pinned to its provider.
- A per-provider switch in Cuckoo Control, and an encrypted secrets store
  (`identity::secrets::Sealer`, AES-256-GCM with a label as associated data).

Gaps found while planning:

1. **An unconfirmed write freezes the account.** `control_finish` marks a write
   `uncertain` when the provider does not confirm it. Every later write on the
   same connection is then refused ("An earlier write outcome needs
   reconciliation", `postgres/control_actions.rs`, two places). No code path
   moves a control action out of `uncertain`. Reconnecting keeps the same
   connection id, so by reading the code the block survives a reconnect too.
   One timed-out Gmail send can stop every later email from that workspace.
2. **Composio was never run against a live account.** Tool and field names were
   checked against published pages only. A wrong name fails safe, but silently.
3. **A second provider cannot be added for an existing app.** Composio's
   `apps()` lists every catalog app with a `composio` entry, and
   `connectors::apps` refuses when two providers list the same app id. Adding a
   `nango` entry to Gmail today breaks the Connections page.
4. **Reconnecting resets rules,** because Cuckoo cannot prove the new account is
   the same upstream account.
5. **Plain fields only.** cc, bcc and attendees are dropped.
6. **Triggers** still come from the operator event-source file.
7. **Connections has no search or categories.** Fine at 4 apps, not at 400.

## Order of work

Each plan below is one reviewable increment with its own schema number, tests
and runtime doc update. Estimates are engineer-weeks for one person who knows
the codebase, including tests and docs.

| Plan | What | Depends on | Estimate |
| --- | --- | --- | --- |
| C1 | Unconfirmed writes: resolve, reconcile, unblock | none | 1 |
| C2 | Live Composio check and nightly drift report | none | 1 |
| C3 | Routes per workspace and stable account identity | none | 1.5 |
| C4 | Contract v2: capabilities and list fields | C3 | 1 |
| C5 | Nango provider | C3, C4 | 2 |
| C6 | Token vault and native Google Calendar, then Gmail | C1, C3, C4 | 4, plus Google review in parallel |
| C7 | Provider events through the shared events intake | C3 | 2 |
| C8 | Pipedream provider | C3, C4 | 1 |
| C9 | Connections catalog, one-step connect, provider health | C3 | 2 |

Suggested calendar for one engineer, with C9 able to move to a second person:

| Weeks | Work |
| --- | --- |
| 1 | C1 |
| 2 | C2, and start Google OAuth app registration for C6 |
| 3–4 | C3, then C4 |
| 5–6 | C5 |
| 7–10 | C6 (Calendar native first, Gmail when Google review allows) |
| 11–12 | C7 |
| 13 | C8 |
| any, from week 4 | C9 |

C1 and C2 come first because they fix real failures in what is already built.
C3 comes before any new provider because of gap 3.

---

## C1 · Unconfirmed writes: resolve, reconcile, unblock

**Why first.** It is a correctness bug in shipped behavior, it is small, and
every later provider makes `Uncertain` more common, not less.

**Build.**

1. A failing PostgreSQL test first: a write ends `uncertain`; the next write on
   the same connection is refused; after a reconnect it is still refused.
2. A person can settle an unconfirmed write: **It happened** or **It did not
   happen**, with an optional note.
   - Who: the agent's current managers and workspace admins, the same people
     who decide Ask first. The requester alone may not, when the action needed
     someone else.
   - State: `uncertain` moves to `completed` (with `settled_by`, `settled_at`,
     `settled_as='happened'`) or to a new `not_sent` state. Both release the
     write hold. Audit record on every settle.
   - API: `POST /api/identity/workspaces/{w}/control/actions/{id}/outcome`
     with `{happened, note, revision}`. A repeat of the same answer returns the
     saved result; the opposite answer conflicts.
   - UI: the existing **Check** button in Activity, Needs review opens the
     request details with the two choices and the exact fields that were sent.
3. A dispatch key per write, saved on `relay_control_actions` before the call
   (hash of action id and the exact fields). Passed to providers that can use
   it (capability `idempotency_key`, see C4).
4. Automatic reconcile where the app can answer. An optional provider method
   `find_outcome(saved, operation, key, arguments) -> Found | NotFound | Unknown`.
   A background recheck reuses the lease pattern already used by the
   communications outbox (`postgres/communications/delivery.rs`): recheck after
   1, 5 and 30 minutes, then leave it to a person. `Found` settles as
   happened, `NotFound` only after the provider says writes are idempotent,
   otherwise it stays for a person.
5. Settings, Connections shows "Writes paused: 1 unconfirmed" on the account,
   with a link to the entry.

**Tests.** PostgreSQL: hold, settle by manager, settle refused for requester
when another approver was needed, repeat and conflicting settles, stale
revision, reconnect does not lose the hold, settle releases the hold. Unit:
recheck schedule. Web: the two choices, double submission, failed save keeps
the note. Journey: `connector_providers.mjs` gains a send whose fake provider
times out; the person settles it and the next send goes through.

**Done when** no unconfirmed write can block an account without a visible way
for the right person to clear it.

**Decision needed.** Confirm the people who may settle (proposed: managers and
admins, same as Ask first).

---

## C2 · Live Composio check and nightly drift report

**Why.** Gap 2. Today the first live call is a customer's.

**Build.**

1. Add `describe(saved, operation) -> provider schema` to the contract. For
   Composio it reads the pinned tool's input schema; nothing is sent to the
   app. For MCP it lists tools.
2. `ayati admin connectors verify [--provider composio]`: for every catalog
   action, compare the provider's current schema with the catalog mapping and
   print `ok`, `missing tool`, `renamed field`, `new required field` or
   `version moved`. Exit non-zero on any failure, so it can run in CI against a
   test project.
3. Run it once against a real Composio project with a Cuckoo-owned Google test
   account. Fix the catalog. Record the date, Composio API version and results
   in `runtime/connector-providers.md`.
4. Nightly in the controller, under the existing single-runner lease: run the
   same check for each connected account and save results in a new
   `relay_connector_drift` table (provider, app, action, status, detail,
   checked). Show the latest report beside the provider switch in Cuckoo
   Control (`web/src/control/providers.tsx`) and as a Needs fixing cause for
   affected agents.

**Tests.** Unit: diff rules for each status. Conformance suite: `describe` on
both providers. PostgreSQL: drift rows replace the previous run per action.
Live: the one recorded run against a real account.

**Done when** schema drift appears in Cuckoo Control before an agent hits a
`Changed` refusal.

---

## C3 · Routes per workspace and stable account identity

**Why.** Gap 3 blocks every new provider. Gap 4 is the most common admin
complaint after reconnects.

**Routing.**

- `catalog.json`: each app keeps a `providers` map and gains
  `default_provider`. Several providers may map the same app.
- New table `relay_connector_routes (workspace, app, provider, changed_by,
  changed, note)`, primary key `(workspace, app)`. Platform-wide overrides live
  in `relay_connector_app_providers (app, provider, ...)`, set from Cuckoo
  Control.
- Resolution: workspace route, then platform override, then catalog default.
  `connectors::apps` and `provider_for` read the resolved provider instead of
  scanning providers, and the "two providers offer" refusal becomes a check
  that the resolved provider is configured.
- A route change affects only the next connect. Existing connections keep the
  provider stored on them, as dispatch already does. Moving a workspace to a
  new provider is therefore: change route, admin reconnects, old account is
  revoked at the old provider.

**Account identity.**

- `Account` gains `identity: Option<String>`: the app's own stable id, read by
  the provider at finish time (Google `sub` or mailbox address from a profile
  read, Slack team and user id, Microsoft tenant and object id). Stored as a
  digest in `relay_control_connections.account_identity`.
- On reconnect, if the identity matches and the reviewed definition (actions
  and fields) is unchanged, keep the agent switches and rules. Still advance
  the connection revision so pending approvals are invalidated and running
  work is fenced, as today.
- Any difference, or no identity from the provider, keeps today's behavior:
  rules start over from safe defaults.

**Tests.** PostgreSQL: route resolution order, route change leaves existing
connection on its provider, same-identity reconnect keeps rules and cancels
pending approvals, different identity resets, missing identity resets, schema
upgrade from 101 with existing Composio connections. Conformance: `identity`
for both providers.

**Decision needed.** Same-account reconnect keeping rules changes a security
rule written in the runtime docs. It needs the owner's approval.

---

## C4 · Contract v2: capabilities and list fields

**Build.**

- `fn capabilities() -> Capabilities` with: `sign_in`, `revoke`,
  `idempotency_key`, `find_outcome`, `triggers`, `personal_accounts`, and
  `field_kinds` (`plain`, `list`). Settings and agent setup read these flags,
  never provider names.
- Field kinds in the catalog: `"cc": {"kind": "list", "max": 20}`. Lists hold
  plain values only, bounded in count and length. `Operation::arguments`
  validates them. A provider without `list` support gets the action without
  those fields, as today.
- Company rules (`guard-rules.md`) can test "list contains" and "list length",
  stricter only.
- Add `gmail.send_email.cc`, `bcc` and `google_calendar.create_event.attendees`.

**Tests.** Unit: list validation and bounds. Conformance: both providers report
capabilities; a provider without lists never receives one. PostgreSQL: a
company rule on list length asks first.

---

## C5 · Nango provider

**Why Nango second.** It is the one provider whose tokens and integration code
can stay under Cuckoo's control later, through self-hosting, and it covers
CRM and accounting apps Composio handles less well.

**Shape.** Use Nango for sign-in, token refresh and its API proxy. Cuckoo
defines each action itself in the catalog as a reviewed HTTP request:

```json
"nango": { "integration": "hubspot", "method": "POST",
  "path": "/crm/v3/objects/contacts",
  "body": { "properties.email": "email", "properties.firstname": "first_name" } }
```

This avoids Nango functions, which need Nango Cloud or its paid self-hosted
edition, and keeps every action Cuckoo-reviewed.

**Integration check first** (nango.dev was outside this session's network, so
these are unverified): connect session creation and the hosted connect page,
how the browser returns to Cuckoo's callback, the auth webhook, the proxy
headers, revoke, and rate-limit responses. Record the tested API version.

**Build.**

1. `connectors/nango.rs`, `nango_api.rs` and `fake_nango.rs`, following the
   Composio files. Settings: `AYATI_NANGO_KEY_FILE`, optional
   `AYATI_NANGO_BASE_URL` for self-hosted or tests. Executors refuse them.
2. Each workspace is one Nango end user, `cuckoo-<workspace id>`, the same
   isolation rule as Composio.
3. `finish` reads the account identity (C3) with a profile request through the
   proxy. `prepare` checks the connection is active and the integration
   unchanged. `describe` returns the catalog request shape plus the Nango
   integration's scopes.
4. First apps on Nango: HubSpot and one accounting app (Zoho Books or
   QuickBooks), chosen by the first customer who needs them.

**Tests.** The full conformance suite against `fake_nango.rs`. PostgreSQL: a
workspace routed to Nango connects and dispatches. Journey: the existing
connector journey, run once with Gmail routed to Nango against the fake.

**Decision needed.** Nango Cloud to start (recommended), or self-hosted now.

---

## C6 · Token vault and native Google

**Why.** Google Workspace is the most-used set of apps, the one where provider
fidelity matters most (threads, attachments, attendees), and the one whose
message bodies customers will ask about.

**Token vault.** A small trait, used only by native connectors:

```rust
trait TokenVault {
    async fn authorize_url(&self, app: &str, workspace: &str, callback: &str) -> Result<SignIn>;
    async fn exchange(&self, sign_in: &SignIn, code: &str) -> Result<VaultAccount>;
    async fn access_token(&self, account: &VaultAccount) -> Result<Secret>;
    async fn revoke(&self, account: &VaultAccount) -> Result<()>;
}
```

The first implementation reuses `secrets::Sealer`: refresh tokens sealed in a
new `relay_connector_tokens` table, with `workspace/connection` as associated
data so a token cannot move between rows. No new service. On AWS the sealer
key can later come from KMS. A Nango or AgentCore Identity vault can replace it
behind the same trait if needed.

**Google review is the long pole. Start it in week 2.**

- Register the Cuckoo Google OAuth app, consent screen, privacy policy and
  domain verification.
- In Testing status an External app is limited to listed test users and its
  refresh tokens expire after about a week; plan development around that.
- Scope classes decide the cost. Calendar scopes are sensitive: verification,
  no security assessment. Gmail read and compose scopes are restricted: a CASA
  assessment by a Google-approved assessor, renewed every year. Check each
  scope against Google's current list before choosing.

**Build, in this order.**

1. `connectors/native/mod.rs` (the `cuckoo` provider, routing actions to app
   modules) and `connectors/native/google/` with `auth.rs` (scopes per enabled
   action), `client.rs` (a thin typed client over `reqwest` for the endpoints
   used), `calendar.rs`, `gmail.rs`, `errors.rs`.
2. One error mapping for every native connector: `invalid_grant` and 401 are
   `NeedsReconnect`; a missing scope or changed response shape is `Changed`;
   timeouts and 5xx are `Uncertain`; 429 backs off within the per-workspace
   rate limit.
3. **Calendar native first.** `events.insert` accepts a client-chosen event
   id, so the C1 dispatch key becomes the event id and a repeat is refused by
   Google. That gives exact `find_outcome` and real idempotency.
4. **Gmail send and draft** once the scopes are approved. Set a Message-ID
   derived from the dispatch key and look for it in Sent to reconcile; verify
   in the integration check that Gmail keeps it.
5. **Gmail read and Drive** after the assessment. Prefer `drive.file` with the
   Google Picker over full Drive read where the job allows.
6. Route new Google connections to `cuckoo` per workspace (C3). Keep Composio
   as the fallback route for workspaces not yet moved.

**Maintenance built in from day one.**

- Recorded request and response fixtures for every action; tests replay them.
- A nightly live smoke of each read action against a Cuckoo-owned account, and
  of writes against a sandbox calendar and mailbox, reported like C2.
- Scheduled reminders in Cuckoo Control, with an owner, for the yearly CASA
  renewal, client-secret rotation and Google API deprecation notices.
- A runbook per app in `runtime/`: rotating secrets, reading Google's status,
  what each refusal tells a customer.

**Tests.** Conformance suite against recorded fixtures. Unit: error mapping,
scope selection per enabled action, Calendar id derivation. PostgreSQL: sealed
token round trip, wrong associated data refused, revoke removes the row.
Journey: connect Calendar with a local OAuth stand-in, create an event, time
it out, reconcile it as happened.

**Decisions needed.** Approve the Google OAuth app under Cuckoo's name and the
yearly assessment budget.

---

## C7 · Provider events through the shared events intake

**Why.** Gap 6. Today an operator writes an event-source file per workspace.

**Build.**

- `events(saved) -> Option<EventSource>` with `subscribe`, `renew`, `verify`
  and `unsubscribe`. Created when an admin turns on a trigger for a connected
  account; removed on disconnect.
- Each delivery goes through the existing shared events path
  ([shared-agent-events](../../runtime/shared-agent-events.md)): signature
  before parsing, stable event id, semantic hash, receipt before
  acknowledgement, at most eight scalar fields. Only the source binding
  changes; the authority rules do not.
- Composio: its signed V3 triggers, already verified by Cuckoo. Nango:
  webhooks. Native Calendar: push channels renewed before they expire. Native
  Gmail: polling history ids at first, because push needs a Google Cloud
  Pub/Sub topic.
- Retire the operator event-source file for provider apps; keep it for
  operator MCP servers.

**Tests.** Per provider: valid, replayed, tampered and late deliveries.
PostgreSQL: disconnect removes the subscription; a renewal failure shows in
Needs fixing.

---

## C8 · Pipedream provider

**Why third.** Largest catalog, priced per workspace rather than per call. It
fills gaps; it should never be the only route for an app, because its roadmap
after the 2026 Workday acquisition is not public.

**Build.** `connectors/pipedream.rs`, `pipedream_api.rs`, `fake_pipedream.rs`.
Each workspace is one external user. Catalog entries map to a Pipedream action
component and a pinned component version. Integration check first: connect
tokens and link, account listing and deletion, running an action with a
connected account, and the proxy.

**Tests.** Conformance suite against the fake; one recorded live run.

---

## C9 · Connections catalog, one-step connect, provider health

**Build.**

- Catalog with categories and server-side search and paging:
  `GET …/control/catalog?query=&category=&cursor=`. Apps carry a category in
  `catalog.json`.
- One-step connect for admins: Connect approves the app, opens sign-in and
  applies safe defaults. Approve-only stays available.
- **Request this app** for members, from agent setup step 2 and from the
  catalog. It appears in an admin's Home attention.
- On each account card: provider in small text for admins only, state
  (Connected as, Needs reconnect, Writes paused, Provider off), last used and
  last failure.
- Cuckoo Control, Connector providers: per provider, calls and estimated cost
  this month, error rate by refusal class, last drift report, accounts needing
  reconnect, and a circuit breaker that pauses calls for a few minutes after
  repeated failures instead of holding executor slots.
- A per-workspace, per-app rate limit so one agent cannot use up a customer's
  API quota.

**Tests.** Web: search, empty and error states, keyboard use, phone width,
light and dark, double submission on Connect and Request. PostgreSQL: request
visibility, breaker opens and closes, rate limit per workspace. Browser check
of the changed journeys.

---

## Not in these plans

- Native Slack, Microsoft 365 and WhatsApp Business. Same shape as C6, each
  when a customer needs it. Slack also replaces the Communication MCP bridge
  for Slack messages.
- Personal accounts ("send from my own Gmail"). C3 identity and C4
  capabilities prepare for it.
- Vendor-hosted MCP servers with OAuth sign-in (Atlassian, Notion, Linear,
  GitHub) as a route through the existing `mcp` provider. Small once C3 lands.

# Native communication foundation

This local implementation adds a common communication envelope and PostgreSQL
inbox/outbox to the existing controller. Chat records ordinary message intake and
successful final reply publication. Email and WhatsApp are **local simulations
only**. No mailbox, number, webhook, provider account or cloud resource is created.
This is a selected foundation for D219, not completion of NC2–NC6.

## Incoming messages and context

Chat uses its existing authenticated session, conversation and agent permissions.
A simulated external event requires the workspace admin and a server started in
`simulate` mode. Normal identity mode returns 404 for simulation mutations; a
request field/header cannot enable them.

An endpoint binds one workspace, agent, transport and address. The admin explicitly
maps a simulated sender to an existing authorized workspace person. This is a
fixture assertion, **not real email/phone verification or support for arbitrary
external customers**. A message cannot supply a requester or choose another tenant.

Intake deduplicates endpoint/event identity against the entire envelope's hash.
Changed retries are rejected. Unknown, disabled or unauthorized senders are held
without agent work; autoreplies and our own sender echoes are ignored. Held/ignored
bodies and addresses are not retained, only event and hash. Binding a sender later
does not automatically execute held messages.

Accepted events use private conversations keyed by endpoint, sender, provider
thread/conversation and endpoint/binding generations. The same employee has
separate Chat, email and WhatsApp contexts. Changing endpoint/binding state advances
its generation: re-enabling does not revive old context or sends. These conversations
cannot become public, acquire members, or accept ordinary Chat/schedule/handoff input.

The normalized message, ordinary agent task and immutable inbox receipt commit
together before acknowledgement. Existing worker/Goose execution uses the bound
person's actual current profile and scoped context. Request metadata identifies
the communication transport, endpoint, event and simulation status.

Successful final completion commits an exact outbox envelope. Chat's `published`
state means saved to the conversation, not delivered/read by a human. Simulated
external replies are queued for explicit fixture dispatch. Before dispatch, the
controller checks endpoint, binding, audience, membership, agent access, original
source, context generation and exact current reply content again.

No historical messages are backfilled. Background/helper requests retain their
existing provenance. Failed/cancelled tasks and intermediate progress do not send
external messages in this slice.

## Delivery and recovery

Database triggers protect the exact sender, recipient, conversation, body, source
task and claim. Inbox receipts and routing identities are immutable too. Only
endpoint/binding state/revision and delivery attempt fields can change.

An explicit dispatch saves `sending`, an attempt ID and a 30-second lease before
the simulated provider outcome:

| Fixture outcome | Saved state | Next action |
| --- | --- | --- |
| `accepted` | `accepted` | Await receipt |
| `delivered` | `delivered` | Complete |
| `rejected` | `failed` | Explicit retry, at most three attempts |
| `timeout` | `uncertain` | Reconcile; never automatically resend |
| `crash` | `sending` | After expiry, next dispatch marks it uncertain |

An uncertain attempt can accept a matching acceptance/delivery receipt, but cannot
be retried through the API. Fixture receipt identity is the exact attempt plus
`sim:<outbox-id>:<attempt-id>`. Late acceptance cannot downgrade delivered state.
An already attempted send may complete after revocation; its receipt records what
happened and grants no new authority.

There is no background external dispatcher. Fixture dispatch/reconciliation are
explicit. Real providers need authenticated callbacks, reconciliation and service
recovery; local tests do not prove exactly-once external delivery. Retention/erasure
workflows remain needed: protected immutable receipts currently retain accepted content.

## Local API

Prefix: `/api/identity/workspaces/{workspace}/communications`. All operations use
the existing session/Origin contract and require admin access. Status exposes
bounded operational metadata, never private message bodies.

| Method and suffix | Payload / result |
| --- | --- |
| `GET` prefix without trailing slash | Last 100 inbox/outbox metadata, endpoints, capabilities |
| `POST /simulation/endpoints` | `{id,agent,transport,address}`; email or whatsapp |
| `POST /simulation/endpoints/{id}/bindings` | `{sender,user}` |
| `POST /simulation/endpoints/{id}/state` | `{revision,enabled}` |
| `POST /simulation/endpoints/{id}/bindings/{sender}/state` | `{revision,enabled}`; URL-encode sender |
| `POST /simulation/endpoints/{id}/incoming` | Envelope below |
| `POST /simulation/outbox/{id}/dispatch` | `{outcome}` from table above |
| `POST /simulation/outbox/{id}/receipt` | `{attempt,provider_id,delivered}` |

```json
{
  "event": "fixture-email-1",
  "sender": "employee@simulation.invalid",
  "recipient": "assistant@simulation.invalid",
  "conversation": "fixture-thread-1",
  "text": "Please review my onboarding steps."
}
```

Unknown JSON fields are rejected. Text is limited to 12,000 bytes; external
attachments, CC/Bcc and group audiences are unsupported. HTTP bodies are limited
to 32 KiB. A workspace has fixture bounds of 100 endpoints, 1,000 bindings and
1,000 external inbox events, plus existing task queue limits. These are simulation
bounds, not billing entitlements. No new React screen is included. Bound people
can see their simulated conversation in existing Chat; input must use the fixture API.

## Extending the system

Provider-specific authentication, payload normalization and delivery rules belong
outside the core inbox/context system. Keep the first adapters inside the Rust
controller; adding a transport does not require another agent brain or memory.

A real adapter must derive the tenant from a verified registered endpoint, verify
the provider event, normalize a bounded envelope and establish a real sender with
appropriately limited capabilities. Production external customer principals are
not implemented. Never turn the fixture admin assertion into production identity proof.

Email still needs thread/recipient rules, HTML/attachment sanitation, mailbox
reconciliation and an approved outgoing sender. WhatsApp needs business onboarding,
eligibility, consent, messaging-window/template checks and authenticated receipts.
Live dispatch must be separately implemented, not a switch on a simulated endpoint.
Organization/agent rules, exact approvals, spending checks, suppression, handover,
proactive messages, external progress and connection UI remain planned. Incoming
text saying yes is never an action approval.

## Checks

Schema 32 is forward-only. Use a disposable database, never the normal demo database.

```sh
RUST_TEST_THREADS=4 cargo test --lib communications_ -- --ignored
python3 tools/run-agent-load.py --suite communications
```

The first command requires `RELAY_TEST_DATABASE_URL`. The bounded runner creates
private PostgreSQL, exercises real controller/Goose processes with a scripted
provider, retains `native-communications.json` and logs, and cleans up owned
processes. It uses no live messaging or paid inference.

## Verified local checkpoint — 2026-10-26

- 261 PostgreSQL tests passed, including ten new communication cases and the
  populated schema-31 upgrade. The unchanged S3-specific storage test was not run.
- 104 root and nine guest unit tests passed. Clippy passed for all workspace targets.
- The native communication process scenario passed with real Goose, synthetic
  email/WhatsApp events, controller restarts, separate employee/transport context,
  uncertain sends, late receipts, revocation and normal-mode API denial.
- All 18 existing workspace process/browser scenarios passed, including restore,
  saved context, sender identity, questions, handoffs, schedules, events and Home.
  Both control integration scenarios passed, including the authenticated embedded
  gateway fixture excluded from the standalone database invocation.
- All 178 web tests, the web build, browser-test type checking and 20 Python restore
  tests passed. Source bounds: 878 files, none above 500 lines.

Final retained bounded runs are `.tmp/bounded-agent-g7w74fcv` (communications),
`.tmp/bounded-agent-_tu0xmz3` (workspace) and `.tmp/bounded-agent-wg4314xd` (control).
Their manifests record the binary/source hashes and resource limits. The cleanup
check retained in `.tmp/communications-cleanup.json` found 21 worker reports,
67 recorded processes and no surviving owned processes. The normal demo remained
running and its database was not migrated. These ignored artifacts are local
evidence, not checked-in fixtures or proof of live-provider/cloud readiness.

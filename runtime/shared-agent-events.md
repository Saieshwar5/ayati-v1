# Shared connected-system events

## Since schema 115: trigger runs happen in work rooms, with a Memory setting

Built 2026-11-14 (step E2 of the owner-approved plan "How work reaches agents", D247).
Event triggers now behave like schedules (schema 56):

- **Work rooms.** Every run happens in a private work room holding the person
  who set the trigger up and the agent. It never appears in Chat, people can't
  write into it, and its runs show in Activity. A run never reads anyone's
  conversation.
- **Memory** (`relay_event_triggers.memory`, part of the saved configuration):
  `fresh` (the default for triggers) opens a new room for every run, so a run
  sees nothing of earlier ones; `remember` keeps one room per trigger
  (`relay_event_triggers.room`), so later runs read the earlier ones. Schedules
  gain the same column (`relay_agent_schedules.memory`), defaulting to
  `remember` as before; a `fresh` schedule opens a new room for each run and
  keeps no lasting room. Each delivery records the room it ran in
  (`relay_event_deliveries.room`). Rooms open inside the intake transaction, so
  a refused or full-queue run leaves none behind.
- **Where results go.** A trigger's conversation (`chat`) is now optional and
  means "also post the result to", as for schedules. With none, results stay in
  Activity. When one is set, a completed run posts the trigger's name, the
  first 400 characters and a `[View run](run:<id>)` link, recorded in
  `relay_run_posts`. Existing triggers keep their conversation as the post-to
  target and start fresh.
- **Authority.** Starting a run needs what a schedule needs: the person looks
  after the agent and may use it, and may post where the trigger posts, if
  anywhere. The source read rule (an agent **Allow** on the read action) is
  unchanged.
- **Posted results follow their run.** A posted summary carries the run's
  result, so history, search and the live stream hide it when the viewer may
  no longer see the run (`work_rooms::guard_posts`), and later conversation
  context leaves it out unless the run itself could be remembered
  (`conversation_context::source_exclusions`). Both apply to schedule posts too.
- **API.** Trigger inputs (`POST …/event-triggers`, from an app or a reviewed
  source) take optional `chat` and `memory`. Saved triggers report `memory` and
  `chat_name` (null for Activity only). Activity run details gain `trigger`:
  name, source, memory, post target and whether the result was posted.
- **Screens.** The schedule and both trigger forms share one **Where results
  go** control and one **Memory** control (`automation/results-and-memory.tsx`).
  A trigger run in Activity shows *Started by: Event*, *Results* and *Memory*.
  Recent deliveries link to the run in Activity.

Checks: PostgreSQL `automation_memory_tests` (fresh rooms, one remembering room
with a posted result, refused memory values, fresh schedules) and the updated
event, intake and scope tests; web `agent-app-events` and `agent-events`.


Implemented locally on `feat/shared-agent-events`, serially after PR 73.
The plan below was saved before application edits; the acceptance section records
executed evidence and the remaining provider/cloud boundary.

Reuse the existing Composio V3 signature verifier and pinned Rust dependencies.
[The current provider contract](https://docs.composio.dev/docs/setting-up-triggers/subscribing-to-events)
uses signed raw bytes, stable event IDs and explicit account/user/trigger metadata.
No provider subscription, HTTPS ingress or paid service is provisioned here.

## Authority and data contract

- A private operator event-source profile binds one workspace, already approved
  MCP connection, provider user/account/auth-config/trigger/type, reviewed read
  operation and a short list of scalar event fields. Secrets remain in separate
  owned 0600 files. User/model event payloads cannot select these bindings.
- A maintainer explicitly creates a trigger with its original human principal,
  source, task, destination and optional exact scalar filters. The person must
  have current request/destination access and admin/delegated read authority.
  The agent must already have an effective Allow read rule in that exact channel.
  Ask/Block is not automatic permission to publish an incoming private payload.
- Review shows the account/source, included field names, audience and destination.
  Trigger creation never calls a model, grants tools or creates a provider source.
  Immutable source/filter/task configuration changes require pause and a new rule.
- Verify signature/timestamp before parsing; validate the exact V3 account metadata
  and header/body event ID before receipt. Bind rule and receipt to source semantics
  and current connection/read/delegation versions. Unknown fields are not copied.
- Persist a semantic-content hash, minimal receipt and matched deliveries before
  acknowledgment. Identical retry returns the original acceptance; conflicting
  content is rejected. Different IDs with identical data remain distinct events.
  Unmatched/paused deliveries are never rematched when a rule is created/resumed.
- Project at most eight explicitly configured top-level scalar fields, bounded to
  6 KiB total, and keep event data outside ordinary human-message bodies. Only
  controller-prepared source context supplies it to the authorized original task.
  Independent later messages do not silently copy event-derived reply text into
  another model prompt. Original task continuation retains its source guard.
- Add a focused common source-access boundary for existing Library and connected
  events. Apply it before execution, context delivery, progress/output, question/
  approval disclosure, history/SSE, saved readers/downloads, attention and counts.
  Read grant revocation must not leak private data through alternate read paths.
- Source invalidation uses a source/chat-scoped live event, without payload text,
  so affected readers, questions and approvals clear before revalidation. Keep
  unrelated agent setup/forms open. Private-agent and conversation visibility
  remain independent from workspace ownership and request authority.

## Queue and recovery

Receipts, matched deliveries and business jobs remain distinct records. A bounded
controller loop admits pending deliveries through the existing transactional pooled
background intake, with one unfinished run per rule and existing queue limits.
Retain individual events rather than dropping them during a long question/approval
wait. Commit delivery/job/source provenance atomically across controller replicas.

Pause cancels pending and unstarted initial deliveries; running/saved continuations
keep their own Stop. Only the original principal may resume under current authority.
Source/member/grant changes durably block future rules and cancel affected pending
work; restoring access cannot revive old deliveries or stopped tasks. Archive closes
future intake while preserving already accepted task continuation. Redelivery of a
terminal event cannot cause another business job or effect.

Retain minimal deduplication receipts; erase projected input after no delivery or
unfinished task still needs it. Configure finite intake/pending bounds and reject
pressure without partial acknowledgment or cursor loss. No paid inference occurs
while listening, polling or showing status. Actual execution retains ordinary exact
tool controls, independent approvals, lease fencing and uncertain-write handling.

## Implementation sequence

1. Finish source/plan audit and OpenPencil event-rule review/run concepts before UI.
2. Add schema 28 and protected source catalog, signed strict webhook route, exact
   receipts/matches, immutable rules/state changes and bounded admission/recovery.
3. Compose source authorization across alternate read/execution paths and add
   scope-aware live invalidation. Preserve historical migration fixtures and reject
   unsupported schemas at real startup; never migrate the normal workspace DB.
4. Add understandable Automation event choices/review, pause/resume, saved delivery
   history and explicit missing-operator/provider limitations.
5. Prove signature/account/tenant forgery denial, duplicate/conflicting receipt,
   unmatched/paused replay, backpressure/rollback, two-controller admission, source
   revocation/regrant, private agents, original-task context isolation and exact
   output/approval/question/attention/history denial. Exercise restart/Stop and
   real pinned Goose with a local provider/MCP fixture, one independent approved
   write, zero inference on status and narrow/dark UI.
6. Run focused and relevant regression checks, retain evidence and publish a
   single dependent PR after #73. Only then start channel-visible handoffs.

Rich/nested payload extraction, arbitrary webhook formats, real provider delivery/refresh, cloud
capacity/SLA/cost acceptance and channel handoffs remain separate increments.


## Operator configuration and supported API

Schema **28**, `m20261026_000028_shared_agent_events`, adds source receipts,
immutable human-authorized triggers, deliveries and exact state-change receipts.
Current startup/schema grants follow [PostgreSQL workspaces](postgres-workspaces.md).
Apply explicit migrations only to the intended deployment. Tests migrated isolated disposable PostgreSQL schemas.
The native SQLite adapter does not implement these shared event routes.

`AYATI_EVENT_SOURCES_FILE` points to an absolute, owned, regular 0600 JSON file
under 64 KiB. It is an array of at most 50 operator profiles. Each profile has:

| Field | Meaning |
| --- | --- |
| `id`, `workspace` | Source UUID and exact workspace UUID |
| `connection`, `connection_fingerprint` | Approved account UUID and its current reviewed MCP fingerprint |
| `name`, `provider` | Human-readable name and the supported `composio_v3` adapter |
| `user_id`, `connected_account_id`, `auth_config_id` | Exact provider identity expected in metadata |
| `trigger_id`, `trigger_slug` | Exact provider trigger identity/type |
| `read_operation` | Reviewed read operation with effective Allow in this agent's destination |
| `fields` | One to eight explicitly selected top-level scalar field names |
| `secret_file` | Separate absolute owned regular 0600 file holding the literal signing secret |

No secret, file path or provider user/account identifier is sent to the client.
The UI shows the reviewed account label, source, included fields, destination,
principal and filters. Provider-trigger registration is an operator prerequisite;
this feature neither provisions a subscription nor adds a public ingress.
Use a new source/trigger review after account credentials or source semantics
change. Signing-secret rotation alone preserves accepted delivery provenance.
Malformed/missing setup fails intake closed while existing readable triggers can
still be paused. An exact previously saved creation can be acknowledged even
when the operator catalog or pooled execution later becomes unavailable.

The signed endpoint is `POST /hooks/composio/workspaces/{workspace}/{source}`.
It requires `webhook-id`, `webhook-timestamp`, `webhook-signature`; verifies the
exact raw bytes with the existing constant-time HMAC verifier and a ±300-second
window, then checks the complete provider identity and header/body ID. It accepts
only `composio.trigger.message`. It has a 64,000-byte request limit. The signature
route alone has a narrowly matched CSRF exception; browser mutation routes retain
Host, Origin, cookie and request-header checks. No cookie authorizes event intake.

Authenticated trigger APIs live below
`/api/identity/workspaces/{w}/agent-profiles/{agent}/event-triggers`:
list/create; `/{id}` for full reviewed instructions; `/{id}/state` for exact
versioned pause/resume; `/{id}/runs` for the latest 20 matched delivery states.
The UI supports optional exact **text** filters. The API supports scalar equality,
with types preserved. It does not support expressions, rich/nested projections or
model-written event routing. Configuration changes require a separate trigger.

Operational bounds are 100 stored triggers per agent, 1,000 pending deliveries per
workspace and a default 100,000 retained receipt IDs per workspace. Set
`AYATI_EVENT_RECEIPT_LIMIT` to 1–1,000,000 when a different finite intake bound is
needed. This is a technical pressure guard, not a subscription allowance or paid
usage ledger. Rejected pressure receives 429 without a partial receipt; exact
accepted retries still succeed at the ceiling. The shared one-second owned loop
checks at most 20 pending candidates, preserves per-trigger ordering/no overlap,
and uses the existing 32-background/64-total queue limits. These constants do not
establish cloud throughput or a deadline guarantee.

## Provider events (schema 109)

Built 2026-11-08 for the [connector plan](../designs/connector-layer/README.md)
phase P4. Event sources now also come from **provider subscriptions** the
controller owns, beside the operator file. The trigger, receipt, matching and
delivery code above is unchanged: a subscription is one more `Source`.

- **Catalog events.** `connectors/catalog.json` names app events under Cuckoo's
  ids: `gmail.new_email` (Composio `GMAIL_NEW_GMAIL_MESSAGE`) and
  `google_calendar.new_event` (`GOOGLECALENDAR_GOOGLE_CALENDAR_EVENT_CREATED_TRIGGER`),
  each with seven scalar fields mapped to the provider's payload names. Slugs and
  fields come from Composio's toolkit pages; they were not checked live.
- **Creating a trigger from an app.** `POST …/event-triggers` with
  `{id, connection, event, chat, name, instructions, connection_revision,
  read_revision, delegation_revision, filter}`. The checks are today's: the person
  may ask the agent in `chat`, and the agent has an effective **Allow** on the
  app's read action in that destination. The read action is the app's first
  catalog read the account offers (Gmail: Find emails; Calendar: Find calendar
  events), because the event carries what that read could fetch. Then the
  controller shares the account's active subscription of that event or
  subscribes at the provider outside any transaction, and saves subscription and
  trigger in one transaction. A failed save removes the new provider subscription;
  an exact repeat returns the saved trigger without calling the provider.
- **List.** `GET …/event-triggers` gains `apps`: each connected app with events,
  its `state` (`ready`, `reconnect`, `unavailable`, `no_read`, `needs_allow`), a
  `detail` saying why, destinations and current authority.
  `GET /api/identity/workspaces/{w}/apps/{connection}/events` lists the app's
  events (`{events:[{id,title,fields}]}`) for any member.
- **Removing.** `POST …/event-triggers/{id}/state` with `remove: true` (and
  `enabled: false`) removes a trigger for good. Removing the last trigger of a
  subscription marks it removed; the controller then deletes it at the provider
  (right away, and retried by the admission loop with backoff up to an hour).
- **Disconnect and reconnect.** Disconnecting removes the account's
  subscriptions. Reconnecting with a different provider account breaks them; as
  before, every trigger on the connection is blocked, and its detail now says why,
  so Needs fixing shows "event triggers stopped" with that reason. Create the
  trigger again to subscribe the new account.
- **Intake.** `POST /hooks/composio` (one per deployment) verifies the project
  signing secret, then routes each event by its Composio user
  (`cuckoo-<workspace>`) and trigger id to the saved subscription. A wrong
  workspace, account or trigger is 401, an unknown subscription 404.
  `composio.connected_account.expired` marks every connection bound to that
  account for reconnecting. `POST /hooks/pipedream/{subscription}` opens the
  subscription's sealed signing key and hands the delivery to the Pipedream
  provider (404 while it isn't configured). Both keep the 64,000-byte limit, the
  ±300 s window, receipt-before-200, duplicate and conflicting-retry handling and
  the narrow CSRF exception. Only mapped fields are kept: lists, objects and
  missing fields become null and long text is cut to fit 2,000 bytes per field
  and 6 KiB in all.
- **Setup.** A platform admin runs
  `POST /api/identity/admin/connector-events/composio`. It needs
  `AYATI_PUBLIC_ORIGIN` and `AYATI_SECRETS_KEY_FILE`, registers
  `AYATI_PUBLIC_ORIGIN/hooks/composio` as the project's one webhook for
  `composio.trigger.message` and `composio.connected_account.expired`, and keeps
  the returned secret sealed in `relay_connector_event_setup`. `GET` says whether
  it is set up, never the secret. Until then the agent page says app events
  aren't set up.

Schema 109 (`m20261108_000109_provider_events`): `relay_connector_subscriptions`
(workspace, id, connection, catalog event, provider, provider subscription id,
config and hash, field map, the account's binding, sealed signing key, state
`active`/`broken`/`removed`, provider clean-up, created by and when; one active
per account, event and config), `relay_connector_event_setup`, and the
`removed` trigger state. Checks: `connectors::triggers_conformance_tests`,
`events::provider_events`, PostgreSQL `event_subscription_tests` and
`event_provider_intake_tests`, web `tests/agent-app-events.test.mjs`.

Not yet: live Composio delivery, nested payload fields, per-trigger provider
settings (catalog events use the provider's defaults), and moving operator file
sources to subscriptions.

## Verified local acceptance

- 100 Rust unit checks, 198 disposable PostgreSQL checks, 154 web checks,
  52 offline Python checks. Fourteen PostgreSQL event cases cover replica races,
  exact replay, pressure rollback, source projection, tenant/account forgery,
  delegated Allow versus Ask, original-person resume, key/binding changes,
  populated-v27 migration, archive, Stop, revocation/regrant and alternate reads.
- A real pinned Goose process with the local scripted model asks an ordinary
  question, releases its worker, survives controller/executor restart, then asks
  for a separate exact approval and performs one confirmed authenticated MCP
  fixture write. The run uses eight local scripted model requests; no provider
  subscription, real model inference or cloud service is activated.
- The same journey checks signed envelope rejection, duplicate/conflicting IDs,
  pre-activation/unmatched receipt non-rematching, task-only source context,
  independent-prompt isolation, immutable old results after read revocation and
  zero model calls for status/history reads. Desktop and phone/dark UI were
  captured and visually inspected. Delayed pre-revocation reader responses are
  fenced in web behavior checks.

Evidence is retained in `.tmp/shared-agent-events/` and the process fixture
record identified in `verification.json`; the tracked design folder contains
OpenPencil concepts and actual local UI captures. Final check totals and retained
process records are recorded there after the complete regression run.

This is local acceptance. Real Composio delivery/subscription registration,
provider retry behavior, TLS/ingress, refresh, cloud cost/capacity and production
operations still require separate bounded verification. [Channel handoffs](channel-agent-handoffs.md) extend these tasks with source-
protected helper work as a separate serial increment.

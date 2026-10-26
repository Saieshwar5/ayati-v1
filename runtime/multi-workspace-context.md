# Repeated local context and recovery rehearsal

`tests/multi_workspace_context.mjs` exercises 50 managed agent profiles across
three workspaces owned by three distinct authenticated people. Their display names
are deliberately identical. A second person also uses the first workspace's agent
in a separate private conversation, giving 51 independent conversation scopes.

The controller admits at most six running assignments, two per workspace. Three
two-slot executors use actual pinned Goose against a scripted loopback provider.
Each wave sends one request per conversation and inspects every actual model input:
exact request-message identity, workspace, agent, sender ID/email, current name,
attributed original request and only that conversation's private marker. Every
healthy task must have one attempt and one model invocation. Reading another
person's DM or another workspace is denied; a revoked agent grant blocks new work.

The second wave holds six running requests and 45 queued requests across a
controller restart. Claims and attempt counts must remain unchanged and all 51
finish once. Optional idle intervals span several normal lease/heartbeat periods;
idle reserve and status work must not invoke the model. Resource sampling is split
before/after controller replacement so it measures live process scopes rather
than attempting to inspect a dead PID. Reports retain each wave and resource phase.

```sh
python3 tools/run-agent-load.py --suite context
# Longer local rehearsal; no real provider or cloud calls:
AYATI_CONTEXT_ROUNDS=40 AYATI_CONTEXT_IDLE_SECONDS=90 \
  python3 tools/run-agent-load.py --suite context
```

Defaults are three rounds without idle delay. Rounds are bounded to 3–40 and idle
intervals to 0–90 seconds; every wave also has a 120-second completion deadline.
The runner copies an immutable application binary, creates an owned private
PostgreSQL cluster and enforces the existing CPU/memory limits. Only owned test
processes and infrastructure are stopped; the normal local demo is not touched.

## Current evidence

The three-wave smoke run passed 153 requests on 2026-10-26, including the held
restart, private-person separation and denied follow-up. Evidence:
`.tmp/bounded-agent-4ridtqps/` and `.tmp/independent-worker-YReDHR/`.
Its burst completion times were approximately 14.9, 16.4 and 15.5 seconds. These
are whole-wave wall times, not per-request latency or a cloud startup promise.

The controlled fault suite also passed three cases (controller restart, executor
loss, cancellation/revocation), and the mixed suite passed its five cases with
96 chats beside held background work. Twenty broader bounded process regressions
passed after correcting historical fixture assumptions: managed jobs receive the
explicit nine-tool controller surface, and their fresh controlled Goose PID differs
from the generic prepared reserve PID. Tests now verify that actual active children
are reaped and correlate ownership through supervisor incarnation/assignment rather
than the old reserve PID. No owner shell/browser/memory extension is permitted.

Evidence: `.tmp/bounded-agent-19ums66x/` (faults),
`.tmp/bounded-agent-x41iy7na/` (mixed),
`.tmp/bounded-agent-nn_u6b3n/` (20 regressions). Initial failed fixtures remain in
their earlier evidence directories; they were not treated as product success.

The extended 40-wave run is tracked separately and must not be called passed until
its final report and cleanup are verified. This is local transport/isolation and
recovery evidence with synthetic inference. It does not establish real-model
judgment, automatic sensitive-data classification, production tenant isolation,
fifty running workers, cloud availability or acceptable customer-facing latency.

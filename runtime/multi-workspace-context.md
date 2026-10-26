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
Each wave signs the three fixture people in through the normal simulation login
route and verifies their stable IDs; no session TTL is extended. Wave two expires
only the disposable schema's sessions and proves old cookies return 401 before
normal reauthentication. This prevents a long rehearsal from failing merely
because its original one-hour cookie expired.

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

The original extended run (`.tmp/bounded-agent-z0qunor1/`) completed 34 waves /
1,734 requests with no idle inference, then correctly received 401 when its
fixture login expired after an hour. It is a failed 40-wave run, not a pass.
Its immutable binary predates the host-hint isolation and context-preview changes.
All seven owned scopes were verified inactive afterward.

The corrected current-backend run passed **40 waves / 2,040 requests** with
30-second idle intervals on 2026-10-26, 12:19–12:50 UTC. It retained 120 normal
sign-ins, three expired-cookie denials, the held controller restart, final grant
revocation, exact requester/private markers in every model input, one attempt and
one inference per healthy task, and zero idle inference. Managed host-guidance
exclusion is also asserted in every input. Evidence: `.tmp/bounded-agent-pq4q50is/`
and `.tmp/independent-worker-KgKNk9/`.

All resource phases reported zero OOM kills. Largest scope-lifetime charged-memory
peaks were controller 45.4 MiB, largest individual executor 409.2 MiB, PostgreSQL
207.7 MiB and driver 182.7 MiB. These are observations from this debug/scripted
workload, not additive dedicated allocations or production sizing. The fixture
reported zero cleanup errors; all seven recorded scopes were independently
verified inactive. This run started before the runner added structured cleanup
records, so its cleanup proof is the independent post-run state check. This is local transport/isolation and
recovery evidence with synthetic inference. It does not establish real-model
judgment, automatic sensitive-data classification, production tenant isolation,
fifty running workers, cloud availability or acceptable customer-facing latency.

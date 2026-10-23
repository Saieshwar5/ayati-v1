# Browser testing and agent feedback

## Temporary assignment workers

The backend [assignment worker contract](../runtime/assignment-workers.md) describes
the separate real-Goose/PG process rehearsal. With a dedicated local
RELAY_TEST_DATABASE_URL whose database starts with relay_simulation_:

~~~sh
cargo build --locked --bin ayati
node --test --test-concurrency=1 tests/assignment_workers.mjs
~~~

It checks startup from zero workers, one-assignment exit, ownership renewal,
competing launchers, cancellation/revocation and process-loss recovery. Set
AYATI_ASSIGNMENT_SOAK_SECONDS=185 to cover the former three-minute ACP limit.
The provider is scripted and all database records are disposable. This does not
test AWS, paid models, browser tools or business-effect checkpoint continuation.

## Prepared shared executors

The [executor pool contract](../runtime/executor-pool.md) describes M1.5b. With the
same dedicated disposable database and real pinned Goose, run:

~~~sh
node --test --test-concurrency=1 tests/executor_pool.mjs
~~~

It checks prepared PID dispatch, concurrent sessions and independent supervisors,
workspace isolation, capacity queueing, session replacement, controller restart,
executor crash, graceful drain, failed readiness and cancellation. The provider is
scripted; no cloud or paid model is used. Preserve an immutable `AYATI_TEST_BIN`
copy if Cargo will rebuild the binary while process tests are running.

## Durable live replies and local timing

See the [streaming contract](../runtime/reply-streaming.md). After building Rust
and web assets, use the same private disposable database to run:

~~~sh
node --test --test-concurrency=1 tests/reply_stream.mjs tests/reply_capacity.mjs
~~~

These use actual Goose and a scripted model to exercise partial text, browser
send/offline/reload/Stop, controller replacement, dropped output acknowledgments,
interrupted attempts and bounded-output failures. The named 24-sample capacity
journey records raw timings/quantiles and resource observations, then rehearses
held/short work, saturation, delayed executor growth and draining. Proposed latency
target failures remain in its report. Screenshots and JSON stay in retained fixture
directories; this is not a live-provider, cloud or multi-hour benchmark.

## Controlled concurrent load and recovery

See the [load rehearsal](../runtime/load-acceptance.md). With the same dedicated
local database and an immutable `AYATI_TEST_BIN`:

~~~sh
node --test --test-concurrency=1 tests/agent_load.mjs tests/agent_load_faults.mjs
~~~

The default matrix sends 384 measured requests: two repetitions, small/large
scoped contexts, concurrency 1/4/8/16 and four prepared slots. It pins the driver,
controller and executor processes to four CPUs; this is affinity, not a quota or
exclusive host. It records phase traces, prompt sizes, p50/p95/p99, resource
samples and latency-target misses. Separate fault journeys run controller restart
with backlog, real executor lease expiry, and cancellation/revocation beside other
active requests. Reports and failed runs remain in printed fixture directories.
The model is scripted and PostgreSQL remains outside that affinity; these are not
cloud sizing or live-provider performance results.

For the follow-up with fresh PostgreSQL and verified CPU/memory limits, use:

~~~sh
python tools/run-agent-load.py
python tools/run-agent-load.py --executor-cpu 200
python tools/run-agent-load.py --suite faults
python tools/run-agent-load.py --suite regressions
~~~

The [bounded rehearsal guide](../runtime/bounded-agent-load.md) describes required
local tools, resource ceilings, timestamp precision and retained evidence. Each
invocation owns and stops its test cluster/scopes and keeps their files; no
existing database is reconfigured. These commands require a working systemd user
manager and cgroup v2, and fail instead of falling back to unbounded execution.

## Existing browser and agent journeys

The existing journeys below explicitly select the preserved prototype runtime.
They are regression evidence for that harness. The new Goose path is tested with
`cargo build` then `node --test --test-concurrency=1 tests/goose_integration.mjs tests/goose_durability.mjs tests/goose_scheduling.mjs`.
That suite runs pinned real Goose with a local model fixture, including MCP
approvals, session loading, Stop and web interaction; it makes no paid model calls.
Without a VM manifest, that suite tests the host adapter. To test the actual
QEMU/KVM guest, prepare a **disposable** VM using [the setup guide](../README.md),
then run:

```sh
AYATI_VM_TEST_MANIFEST=/absolute/path/to/disposable-vm/vm.json node --test --test-concurrency=1 tests/goose_integration.mjs tests/goose_durability.mjs tests/goose_scheduling.mjs tests/vm_idle.mjs
```

The manifest must identify `local-agent`. This explicitly opts into writing
synthetic Goose sessions and a storage marker on that guest disk; use no owner
data. The fixture creates its own host database/provider, boots and stops the VM,
and uses real Goose, MCP and the web app. The original thirteen VM scenarios also check disk
persistence, forced-command/identity/key rejection, blocked host/metadata network
access, resource observations, provider-error redaction/recovery, and host-crash
termination/recovery without prompt replay. The three scheduling cases bring the
host selection to thirteen and the original VM selection to sixteen. Six further
VM-only cases cover idle release/wake, model/approval leases, off-state Stop,
event arrival during shutdown, Stop during boot and failed boot configuration.
The full real-VM selection therefore has twenty-two scenarios.
Missing VM prerequisites fail; there is no automatic substitution. `AYATI_TEST_BIN` can select a release binary.
The durability scenarios add concurrent receipt retries/conflicts, queued recovery
after crashes/service shutdown, invalidated approvals, unconfigured queue parking,
and a browser reload/retry after a lost HTTP acknowledgement. The MCP scenarios
also assert the recorded tool output and decision. Both use a scripted model and
make no paid requests. Neither validates cloud deployment or general multi-tenant security; see [the VM record](../runtime/local-vm.md).
The default fixture disables idle shutdown; tests/vm_idle.mjs explicitly selects
a one-second grace to exercise transitions. The real product default is 300
seconds. Schedule/event contracts and clock/backpressure policies are documented
in [scheduled work](../runtime/scheduled-work.md).

## Agent browser inside a real guest

This is separate from the prototype/web-chat browser journeys below. Prepare a
fresh disposable VM with `tools/prepare-local-vm.py --browser`, following the
[browser setup and contract](../runtime/browser.md), then run:

```sh
AYATI_VM_TEST_MANIFEST=/absolute/path/to/browser-vm/vm.json node --test --test-concurrency=1 tests/browser_vm.mjs
```

The eight scenarios use actual Goose, MCP, Chrome and KVM. They cover the allowed
tool surface, a synthetic shop, public HTTPS at example.com, blocked host access,
tab operations, takeover/boot/Stop races, credential-entry separation, restart,
saved cookies, owner pointer/text input, file bounds and private paths, closed-tab
recovery and interaction-aware VM idle release. The provider and shop are scripted;
no paid inference, live business account or real business write is exercised.
The HTTPS scenario requires Internet access. Missing prerequisites fail explicitly.

The suite preserves failed private fixtures. Its owner-panel screenshot is saved
to `.test-runs/browser/owner-panel.png`; it contains only synthetic site data.
Do not use real owner profiles/data for these checks. The existing 22 VM scenarios
also run with an ordinary non-browser guest to check backward compatibility.

The browser tests operate Ayati through its real interface: opening the page,
choosing attachments, typing messages, clicking buttons and downloading files.
They start their own release daemon, SQLite database, workspace and port for
each scenario. Your running Ayati instance and `.ayati` data are not used.
Browser/API observations and the daemon's saved records provide independent
evidence of what happened; an assistant's completion message alone is insufficient.

## Setup and run

Use the project's Linux/Rust/Node requirements from [README](../README.md).
The evidence exporter uses Node's built-in SQLite support; Node 26 is tested.
From the repository root:

```sh
npm ci --prefix web
cd web && npx playwright install chromium && cd ..
cargo build --release
npm run build --prefix web
npm run check:e2e --prefix web
npm test --prefix web
npm run test:e2e --prefix web
```

On supported Linux distributions, `npx playwright install --with-deps chromium`
can install browser system dependencies too. This project was verified with
Playwright 1.63.0's downloaded Chromium on Arch Linux, where Playwright uses its
Ubuntu fallback build. Browser availability on other machines still needs checking.

The default suite uses a local scripted model with fragmented streamed tool calls,
but the actual UI, daemon, storage and Bubblewrap execution. It makes no paid model
calls. One Chromium worker runs at a time; no browser testing dependency is imported
by the shipped web client or Rust daemon.

```sh
# Watch real browser actions, or pause and inspect them in Playwright Inspector.
npm run test:e2e --prefix web -- --headed
npm run test:e2e --prefix web -- --debug --grep 'upload expenses'

# Keep a trace for every selected test while investigating a problem.
npm run test:e2e --prefix web -- --trace on --grep 'Stop'
```

Visible browser runs require a working graphical display. Routine runs are headless.
The initial suite uses geometry, focus, content and downloaded-file checks rather
than screenshot baselines.

## Repeatable scenarios

| Scenario | Observable result |
| --- | --- |
| Expense attachment and analysis | Real CSV download totals 550: Travel 320, Food 80, Supplies 150; original preserved |
| Notes revision and follow-up | New file, original unchanged, useful saved context, browser reload and daemon restart without replay |
| Stop with a queued request | Real shell ends, delayed descendant write never happens, earlier output remains, queued message runs |
| Provider failure and retry | Clear HTTP failure, provider error body absent from chat, next request works |
| Failed message submission | Draft stays intact; keyboard retry creates one saved request |
| Event connection loss | Existing page reconnects after daemon restart without duplicate messages |
| Untrusted markup | Literal content stays text, creates no script/image nodes, executes no script |
| Short laptop and mobile layout | Composer stays in view, no horizontal overflow, Files focus/Escape work, Shift+Enter adds a newline |

The mobile viewport check tests responsive layout, not a physical phone, mobile
keyboard, every browser, or complete accessibility compliance. Real people must
still evaluate comfort, clarity and first-time use.

## Live model scenarios

```sh
# FIREWORKS_API_KEY and optional AYATI_MODEL / AYATI_MODEL_BASE_URL
# come from the environment or your local, ignored .env.
npm run test:live --prefix web
npm run test:live --prefix web -- --headed --grep 'expenses'
```

Live mode is explicit, makes billed online model calls and sends the synthetic test
messages/files to the configured provider. Missing credentials fail before tests
start; the suite does not silently skip live validation or substitute a fake model.
The two initial scenarios check actual expense-file results and follow-up context,
and notes revision with original preservation and retained factual terms. Check
the downloaded revision yourself for meaning and quality. Passing these scenarios
does not establish general agent intelligence, safe handling of every document
instruction, or capabilities not implemented in Ayati.

Live tests have a longer timeout, use outcome checks rather than exact reply wording,
and have no automatic retries. Each case gets a fresh conversation/database.

## Read the evidence after each run

The command prints the run directory. Open its `summary.md` or:

```sh
npm run test:report --prefix web
```

That command opens the latest run's HTML report. In a failing scenario, inspect
the attached evidence, screenshot and trace; [Trace Viewer](https://playwright.dev/docs/trace-viewer)
shows browser actions, DOM snapshots, console messages and network activity.
Successful scenarios retain small summaries, database evidence and selected
review screenshots. Video is off. Use `--trace on` when a complete successful-run
trace is useful.

| File | Contents |
| --- | --- |
| `manifest.json` | Run ID, fixture/live mode, model, Git SHA, dirty-tree flag, tested tool versions, start/end and process outcome |
| `summary.md`, `summary.json` | Overall result, each scenario, every retry attempt, errors and evidence links |
| `runner.log` | Bounded test-runner output, including failures before scenario reports could be created |
| `html/`, `playwright.json` | Standard browser report and detailed Playwright results |
| `results/<scenario>/evidence.json` | Expected/actual checks, messages, task state, saved model exchanges, tool inputs/results, artifacts, browser observations and timings |
| `results/<scenario>/daemon.log` | Bounded daemon output, with the configured provider key redacted |
| Scenario downloads/images/traces | Actual files inspected, screenshots for review, and failure traces |

Browser/network timings are observations at the test boundary, not precise internal
model/tool timings. `model_turns` counts saved assistant response attempts, including
failed attempts. Token usage and model cost are explicitly `null` because the current
provider adapter does not expose them. Private reasoning fields are excluded from
the JSON evidence. Export failure leaves diagnostics and fails the test.

Retries, when explicitly requested with Playwright's `--retries`, remain separate
attempts. A test that passes only on retry is marked flaky and fails the run.
For a failure, read the evidence, diagnose the UI/agent/tool/provider cause, make a
focused fix, and rerun that scenario. Add a regression check when it captures an
important failure. Avoid changing assertions merely to obtain a green result.

## Git and retention

Track the testing code/configuration, dependency lockfile, this guide and synthetic
fixtures. Generated `.test-runs`, reports, downloads, traces, browser profiles,
databases, local credentials and authentication state stay out of Git. Reviewed
visual baselines can be added later when a stable view needs pixel comparisons.

The run root is private (mode 0700). At completion the runner retains the latest
20 recognized completed Ayati runs. Active runs, foreign directories and symlinks
are not deleted. Generated reports can contain full task/file contents; the shipped
scenarios use invented data. Review and redact any custom-data report before sharing
it. This setup does not automatically upload evidence or add a telemetry service.

## Sourced memory

`cargo test --locked --test sourced_memory` covers evidence validation, corrections,
forgetting, persistence, retrieval bounds and foreign business/agent/customer denials.
`node --test --test-concurrency=1 tests/goose_memory.mjs` uses real pinned Goose/MCP
and a scripted provider, including owner UI and actual Goose compaction. For VM
checks, set `AYATI_VM_TEST_MANIFEST` to a freshly prepared current guest. The
compaction-config test is host-only and explicitly skipped on the VM path.
See [the contract and limits](../runtime/memory.md).

## Independent agents

`cargo test --locked --test agent_registry` checks durable setup recovery and input
boundaries. VM config unit tests check exact selected-agent identity.
`tests/independent_agents.mjs` requires `AYATI_VM_TEST_MANIFEST` pointing to a fresh
current browser-enabled guest. It creates a second actual VM from the UI, exercises
Goose with memory and browser together, checks cross-agent denials and independent
Stop, then verifies persistence, drafts, mobile layout and interrupted setup/retry.
The provider/site are synthetic; this does not establish live-model quality.
See [the local agent contract](../runtime/independent-agents.md).

## Home and attention

`cargo test --locked --test attention_storage --test runtime_storage` checks
schema-6 migration, durable/idempotent outcome reviews, old failures beyond the
recent-history window, scope rejection, and restart invalidation.

After `cargo build` and `npm run build --prefix web`, run
`node --test tests/home_attention.mjs`. This uses real pinned Goose and Playwright
with a synthetic provider to check Allow/Deny, stale tabs, offline recovery,
request inspection, interrupted work/review persistence and mobile layout.
Screenshots are written to `.test-runs/home/`.

The existing `tests/independent_agents.mjs` VM test also exercises Home with two
simultaneous independent-agent approvals. It allows one, verifies the other stays
pending, stops the second and rejects its obsolete decision. These tests do not
use a live provider or establish model quality.

## Compact workspace UI

After rebuilding the web app and daemon, run `node --test tests/workspace_ui.mjs`.
It checks root Home navigation, explicit agent routes, review expansion across
updates, agent search, details/draft preservation, theme persistence, mobile
modal drawers, Escape/focus return and composer bounds. Screenshots are saved to
`.test-runs/workspace/`. Existing chat journeys now use `/?agent=local-agent`;
the root URL intentionally opens Home. No live provider is used by this test.

`node --test tests/setup_diagnostics.mjs` checks the Settings guide, explicit
provider-test consent, missing runtime/credentials/VM files, unchanged history,
overlap rejection, redirects and sanitized errors against local synthetic providers.

`node --test tests/agent_profile.mjs` covers profile editing with real Goose and
a synthetic provider, including saved-session replacement, preserved history,
name-only continuity, stale/busy rejection and restart. `profile_storage` covers
transactional validation, private stores and migration/session compatibility.

`node --test tests/agent_model.mjs` verifies candidate testing without saving,
selected-model execution, context separation, restart and busy rejection. The
two-agent KVM scenario also verifies model changes before and after restart.

`node --test tests/agent_tools.mjs` checks host tool controls. With
`AYATI_VM_TEST_MANIFEST`, its compatible-VM case verifies enabling tools that
started disabled. The independent-agent VM scenario covers off/on, browser owner
control, restart, preserved memory and isolation from the neighboring agent.

`node --test tests/agent_schedules.mjs` covers the per-agent schedule UI,
one-time/fixed-interval settings, pause/resume, run results, unacknowledged-save
recovery across reload, disconnected controls, mobile layout and restart.
`cargo test --test schedule_history` checks bounded/scoped durable run history.
The owner-creation case in `independent_agents.mjs` checks two-VM schedule
isolation with identical rule IDs and history persistence.

`node --test tests/agents_workspace.mjs` verifies the graphite workspace,
exclusive work panels, Automation tabs, responsive modal/focus behavior, draft
and reading-position retention, theme persistence and 320/390px layouts. The
web-owner case in `browser_vm.mjs` covers real browser authority and input after
panel switching/expansion. `attention_storage` checks private bounded previews.

# Cuckoo

Cuckoo is becoming a local-first business agent workspace powered by Goose, with
Rust control services and a TypeScript web app undergoing a [staged React migration](runtime/react-client.md). The owner can create
independent agents through the UI, each running inside its own real QEMU/KVM
Linux VM with streaming chat, saved sessions, Stop and exact tool approvals. An optional protected browser now adds
shared viewing, owner takeover and saved login profiles inside that VM.
Optional [sourced memory](runtime/memory.md) adds scoped facts, source quotes,
corrections and forgetting through Goose tools and owner controls.
The full workspace is still being built;
cloud deployment comes after local stabilization.

[Shared agent control](runtime/shared-agent-control.md) adds reviewed remote MCP
operations to PostgreSQL-managed agents, with per-conversation Allow/Ask first/Block,
[explicit approvers](runtime/channel-approvals.md) and durable action receipts.
[Saved task continuation](runtime/durable-approval-waits.md) releases execution
while a task waits. [Ordinary questions and forms](runtime/agent-questions.md)
supply missing facts; they never approve a business action.

The [Agents workspace](runtime/agents-team-workspace.md) shows source-visible work,
progress and per-request Stop. [Saved reading](runtime/result-reading.md) keeps
long replies compact and provides exact saved bodies and passive file/image views.
[Personal pins and channel archive](runtime/chat-organization.md) organize Chat
while preserving existing work and closing new intake in archived channels.
These managed workflows are verified locally. Existing private-runtime question
adapters, broader rule predicates and live cloud/provider acceptance remain open.
Current shared-workspace startup requires PostgreSQL schema **24**, explicit
migration/runtime grants and operator-provisioned private MCP profiles for tools.

[Workspace Library](runtime/library.md) adds private pages, shared reference files,
version recovery and source-based private agent questions to PostgreSQL workspaces.
Files use explicitly configured private S3-compatible or local development storage.
See its runtime contract for configuration, access rules and initial release limits.

Optional [Composio connections](runtime/composio.md) support Connect MCP consumer
keys and Platform project keys, with explicit per-agent operation grants through
Settings and Agent Details. Live MCP discovery and a bounded Gmail read passed;
OAuth refresh, new-account consent and VM acceptance remain unverified.
[Event triggers](runtime/composio-triggers.md) add per-agent rules and a signed
Composio receiver; live delivery needs an explicitly configured HTTPS endpoint.

The public app name is Cuckoo. Existing `ayati` commands, `AYATI_*` /
`RELAY_*` configuration, database names and saved data paths remain compatible.
See [Cuckoo branding](runtime/cuckoo-brand.md) for artwork and naming.

## Run the Goose-backed chat

Requirements: Linux x86_64, Rust/Cargo, Node.js 26, curl, tar, and sha256sum.
The pinned upstream Goose binary is downloaded separately and is not stored in Git.

```sh
./tools/install-goose.sh
npm ci --prefix web
npm run build --prefix web
# Only create this file if you do not already have local configuration:
cp -n .env.example .env
# Edit .env locally: configure FIREWORKS_API_KEY and a model your account supports.
cargo run -- serve
```

Open **http://127.0.0.1:8765** for Home. Choose **Agents** to chat. Without
provider configuration, the workspace remains available and sending is disabled. Restart the daemon after changing configuration.
The binary must match pinned Goose 1.54.0; the installer verifies the release
archive's SHA-256. Release/source details are in [the runtime manifest](runtime/goose-release.json).
This is a real upstream runtime through the official ACP Rust SDK, not linked
Goose internals or a modified fork. Goose and both SDKs use Apache-2.0 licensing;
retain applicable notices when packaging/distributing them.

`AYATI_GOOSE_PROVIDER=fireworks-ai` uses `FIREWORKS_API_KEY`. Alternatively choose
`openai`, set `OPENAI_API_KEY` locally, and choose a matching `AYATI_MODEL`.
Other providers have not been wired into this initial configuration boundary.
`OPENAI_HOST`/`OPENAI_BASE_PATH` can explicitly select an OpenAI-compatible test
endpoint; the integration tests use a loopback fixture with a placeholder key.
`OPENAI_BASE_PATH` is the complete path, such as `v1/chat/completions`.
Legacy `AYATI_MODEL_BASE_URL` affects only the prototype runtime.

The default data directory is **.ayati-local**, separate from preserved `.ayati`
and `.ayati-openclaw` data. It contains Ayati SQLite state, a workspace, and a
private Goose root. No old private data is automatically imported. A second
process cannot use the same data directory. Run commands from the project root.

```sh
cargo run -- status
cargo run -- stop
# Optional release build:
cargo build --release
./target/release/ayati serve
```

`serve`/`start` run in the foreground. Closing the browser does not stop a run.
The UI's Stop cancels a request. The CLI's stop shuts down the service and its
owned runtime, preserving queued jobs for the next configured startup. Started
jobs become interrupted. Completed external actions cannot be undone by cancellation.

## Home and attention

Choose **Home** in the left rail, or open `http://127.0.0.1:8765/?view=home`.
See active work and approvals across agents, inspect individual requests, approve
or deny an exact action, and stop work. Failed/interrupted requests remain until
**Mark reviewed**; their history stays saved. Home refreshes automatically and
rejects stale decisions. **Agents** returns to conversations and agent creation.
Home is now the landing page. The compact rail also opens **Apps** (not yet
implemented) and **Settings**, with theme selection and local setup status.
Agents has search and an optional details panel; narrow screens use drawers.
See [the workspace interface](runtime/workspace-interface.md) and
[Home contract](runtime/home-attention.md) for behavior and recovery details.

## Durable jobs and recovery

SQLite now owns the bounded queue (32 pending jobs), input receipts, execution
claims and state transitions. New queued jobs recover automatically after a crash
or service restart when the runtime is configured. Startup can therefore boot the
VM and call the model for accepted work without a new chat message. Jobs that had
already started remain interrupted; they are never automatically submitted again.
Stop stays final across restarts. VM Stop explicitly cancels queued jobs too.

The web composer keeps a request ID for an unacknowledged message. Retrying the
same draft, including after a reload when tab storage is available, returns its
existing job instead of creating another. Editing the content creates a new ID.
API clients can supply a UUID `request_id` to `POST /api/messages`; the same ID
with different content returns 409, and a full queue rejects new work with 429.
Clients omitting an ID create a new job on each request.

Goose tool observations, owner decisions and bounded results are recorded at
`GET /api/tasks/{id}/actions`. Unconfirmed outcomes stay uncertain; an assistant's
success text cannot override an unresolved action. These are runtime observations,
not authoritative receipts from business services. A lost-reply write is tested
with an independent synthetic service record; live connectors and automated
reconciliation are later work. See [the recovery contract](runtime/durable-work.md).

Ayati database schema is now version 10. History and saved Goose mappings are
preserved; legacy queued jobs from earlier schemas become interrupted rather than
being replayed. Older binaries must not open the upgraded database. Automatic
scheduling and idle VM shutdown/wake are implemented in the next section.

## Scheduled work and idle VMs

The host daemon accepts deduplicated local events and saves one-time/fixed-interval
schedules outside the VM. Missed intervals coalesce to one catch-up job, with at
most one pending occurrence per routine. Queue acceptance and occurrence cursor
advancement are transactional. This first version provides developer APIs; the
guided schedule interface and public business connectors come later.
Configured schedules/events can make billed model calls without an open chat page.

In VM mode, `AYATI_VM_IDLE_SECONDS=300` releases an idle guest after five minutes;
0 disables automatic shutdown. Active model/tool work and pending approvals keep
their VM lease. New work wakes the guest and loads its saved Goose session. Intake
during shutdown stays queued until it can boot again. The host must remain on.

Explicit VM Stop also persists a dispatch pause, preventing timers/events from
immediately waking it. Events remain accepted while paused; schedules remain due.
Owner VM Start or a new accepted chat resumes dispatch. A retry of an old terminal
chat cannot undo Stop. See [the schedule/event API and lifecycle contract](runtime/scheduled-work.md).

## Run the agent in its own VM

The reproducible setup currently targets Linux x86_64 on Arch/Omarchy, with usable
`/dev/kvm`, Python 3.11+, `bsdtar`, `ssh`, `ssh-keygen`, and `/usr/bin/socat`.
It downloads pinned artifacts into the project cache without installing system
packages or changing host services. Other distributions have not been validated.
Allow disk space for the downloads/build and an 8 GiB sparse guest disk; the VM
uses one vCPU and 768 MiB RAM. No software-emulation fallback is selected.

After installing Goose with the command above:

```sh
python3 tools/install-local-vm.py
RUSTFLAGS='-C target-feature=+crt-static' cargo build --locked -p ayati-guest --target x86_64-unknown-linux-gnu --release -j 2
# Preparation creates a NEW private directory; it refuses to overwrite an old VM.
python3 tools/prepare-local-vm.py --directory .ayati-local/vms/local-agent
```

Configure these values in your local `.env`, keeping the provider key there:

```dotenv
AYATI_EXECUTION=vm
AYATI_VM_MANIFEST=.ayati-local/vms/local-agent/vm.json
# Use a fresh control database when switching from an existing host session.
AYATI_DATA_DIR=.ayati-local/vm-control
```

Run `cargo run -- serve`, then open the same web chat. Sending the first message
boots the guest. From another terminal, with the daemon running:

```sh
cargo run -- vm status
cargo run -- vm start
cargo run -- vm stop
```

VM Stop cancels current/queued work and requests guest shutdown. A later message
boots it again and loads its saved Goose session. The disk preserves the guest
workspace and runtime state. Idle shutdown, event wake and scheduled wake are
supported. Starting Ayati can boot the VM for accepted queued work when dispatch
is not paused; it never replays
an interrupted prompt automatically.
An unexpected host crash terminates the owned VM; interrupted work needs deliberate
follow-up. Forced termination is reported when clean guest shutdown fails.

The bridge accepts authenticated, fixed operations through SSH with pinned host
keys; it exposes no remote shell or selectable host paths. QEMU has no host-folder
mounts, and restricted networking permits only the private SSH and model gateway
forwards. The guest receives an ephemeral gateway token; the real model API key
stays on the host. Browser-enabled guests add the filtered HTTP(S) proxy described
below; ordinary guests retain only SSH/model connectivity.
The model gateway supports the configured Fireworks/OpenAI chat-compatible
provider and one selected model. Business connector credentials stay in the
separate host-only [Composio integration](runtime/composio.md).

Leaving `AYATI_EXECUTION` unset (or setting `host`) retains the supervised host
mode for adapter development. VM errors never fall back to host execution.
Provider/model/tool/execution changes cannot reuse an incompatible saved session;
there is no automatic host-to-guest session migration. See the
[VM integration record](runtime/local-vm.md) for pins, checks and limits.

## Browser inside the agent VM

The optional browser slice adds pinned agent-browser/Chrome, a private persistent
profile under a separate guest account, an allowlisted MCP broker and an owner
browser panel. Each agent browser call requires Allow once / Deny. Take control
interrupts the active turn and pauses agent browser actions/captures and queued
work until explicit handback. Closing the panel does not hand control back.
Unfinished form/transfer interaction keeps the VM awake. When finished, select
**Close browser session** or approve the agent's close tool to allow idle release.
Handback preserves live state; an interrupted task needs a follow-up message.

Prepare a **new** VM with the browser bundle; existing VMs are preserved:

```sh
/usr/bin/python3 tools/install-browser.py
RUSTFLAGS='-C target-feature=+crt-static' cargo build --locked -p ayati-guest --target x86_64-unknown-linux-gnu --release -j 2
/usr/bin/python3 tools/prepare-local-vm.py --directory .ayati-local/vms/browser-agent --browser
```

Set `AYATI_EXECUTION=vm`, `AYATI_BROWSER=1`, the new `AYATI_VM_MANIFEST`, and a fresh
`AYATI_DATA_DIR` in local configuration, then build/run normally. This browser VM
uses 1536 MiB RAM and one vCPU. Browser HTTP(S) uses a filtered outbound proxy;
private/host/metadata destinations and IPv6 are refused. The model key stays on
the host. There is no host-browser fallback or exposed CDP/stream port.

The owner panel shows one JPEG frame per second and forwards mapped pointer,
keyboard and text input to the same guest browser. Use it for login instead of
putting credentials in chat. Browser cookies persist; temporary transfers do not
survive VM shutdown. This is a local development viewer, with the existing
loopback/same-origin protections, not production account authorization or remote
mobile access. See [setup, ownership, transfer APIs and limits](runtime/browser.md).

## Tool approval demonstration

Set `AYATI_PROOF_TOOL=1` before the first message (or select a fresh data directory) and restart. Ask the configured model to use
`ayati_proof__get_demo_business`. Its MCP server returns a fixed synthetic shop
profile, has no external credentials, and exposes no shell or arbitrary file tool.
The chat offers **Allow once** and **Deny**, tied to the exact pending action.
This is test data, not a connected store or a business connector implementation.

No extensions are selected by default. The adapter uses Goose 1.54's explicit
extension selection, validates the observed tool identity/arguments before asking
for approval, and never offers an always-allow option. Unknown requests fail closed.
Pending approvals are cancelled on Stop/restart; used decisions cannot be replayed.
The harmless proof server is not a general authorization broker for future tools.
Use Agent Details to change models safely with fresh context. Provider/tool
changes outside the editor still require restoring the original settings or a
fresh explicit AYATI_DATA_DIR; incompatible saved sessions fail closed. Compatible tool controls are available in Agent Details; guided preparation of
incompatible environments remains future work.

## Current limits

- One local business with independent agents, each with its own conversation,
  bounded queue and one running request. Multi-user authentication is not built.
- Agents can use separate real local VMs. The host adapter remains available for
  development; production multi-tenant hosting and fleet operations are unfinished.
- Session loading is supported; interrupted prompts and approvals are not replayed
  automatically. Previously queued work is dispatched after a configured restart.
- The prompt has a three-minute limit and text output is bounded to 1 MiB. A
  waiting approval can expire. Failures preserve text and require deliberate follow-up.
- Goose owns working context and private runtime sessions. Ayati owns run records,
  pending decisions and UI history. Optional sourced memory is available with
  `AYATI_MEMORY=1`; see [its contract and limits](runtime/memory.md). Owner edits
  stop active work; corrections/forgetting start a fresh working session.
- Browser-enabled VMs support agent browsing and owner takeover. Guided agent
  setup, per-agent schedules, Composio grants, reviewed Home panels and local
  generated Apps have UI controls. External trigger infrastructure is implemented;
  live delivery needs separate provider/HTTPS setup, and Connect MCP does not
  expose its subscription-management controls. Supabase/Vercel hosting, media
  input, agent archive/delete and backup/restore remain unfinished.
- The service binds to loopback with same-origin guards. It is a local development
  proof, not a production tenant/security boundary. Keep the machine running for work.

Provider keys and local state are ignored by Git. Automated Goose checks use real
Goose with a local scripted model; they do not establish model quality or cloud
reliability. Separate live Composio MCP checks verified metadata and a bounded
Gmail read. Never put credentials in chat or agent workspaces.

## Verification

```sh
cargo fmt --all --check
cargo clippy --workspace --locked --all-targets -- -D warnings
cargo test --workspace --locked
npm test --prefix web
npm run check:e2e --prefix web
npm run build --prefix web
cargo build
node --test --test-concurrency=1 tests/goose_integration.mjs tests/goose_durability.mjs tests/goose_scheduling.mjs
# Explicit real-VM suite: use a disposable, prepared VM; it modifies its guest state.
AYATI_VM_TEST_MANIFEST=/absolute/path/to/disposable-vm/vm.json node --test --test-concurrency=1 tests/goose_integration.mjs tests/goose_durability.mjs tests/goose_scheduling.mjs tests/vm_idle.mjs
# Separate browser-enabled disposable VM; modifies its profile:
AYATI_VM_TEST_MANIFEST=/absolute/path/to/browser-vm/vm.json node --test --test-concurrency=1 tests/browser_vm.mjs
./scripts/check-file-lines.sh  # local ignored helper, when present
```

The Goose integration suite requires the pinned binary, built daemon, installed
web dependencies and Playwright Chromium (`cd web && npx playwright install chromium`).
It exercises actual Goose ACP, MCP calls, approvals, persistence, Stop and web
interaction against a local model fixture. No paid model requests are made.
The synthetic fixture is explicit; missing prerequisites fail rather than silently skip.

The previous custom harness remains available only with `AYATI_RUNTIME=prototype`
for comparison/regression tests. That mode requires Bubblewrap and keeps its
original shell/file capabilities. It is not an automatic fallback if Goose fails.
The existing eight Playwright journeys and `tests/daemon_smoke.mjs` explicitly
select it. See [test details](tests/README.md); their live suite tests the prototype.

## Development sequence

`codex/05-agent-browser` starts from merged PR #13 on main and adds the first
local browser/owner-control increment. Earlier Goose/VM, durable work, schedules,
events and idle wake milestones remain preserved.
Finish, verify and commit each increment before creating the next numbered branch from that completed tip.
Do not develop several independent copies of the same planned files. Once a
parent branch is merged, align the next branch with its merged history before
publishing it; numbering alone does not prevent conflicts. Push/PR only when asked.

Read `AGENTS.md`, `proj-docs/README.md`, the local implementation plan and current
status before the next increment. Planning documents and references remain ignored
and local. Keep each source file at or below 500 lines and keep lockfiles tracked.
SQLite remains the initial Ayati database; hosting alone does not require replacing it.

### Independent local agents

With Goose VM execution configured, use **New agent** in the web Agents list to
create another role with its own chat, sourced memory, approvals and private VM.
Preparation runs in the background; failed setup can be retried explicitly.
Switching agents preserves separate drafts. The original Ayati conversation stays
available. This local increment supports four agents total; it does not add hosted
customer/staff authentication or shared app access.

See [independent agents](runtime/independent-agents.md) for prerequisites, API scope,
restart/Stop behavior, private setup logs, browser recovery and remaining limits.

### Guided setup checks

Open **Settings → Local setup** for credential-presence and runtime checks.
The optional **Test provider connection** sends one small synthetic prompt after
explicit consent and may incur provider charges. It never sends agent history
or invokes tools. See [setup diagnostics](runtime/setup-diagnostics.md) for scope
and failure behavior. Restart after local configuration changes.

### Edit an agent

Use **Agents → Details → Edit agent settings** while the agent is
idle. Name changes preserve context; changed responsibilities or models start fresh model
context on the next request while keeping saved chat, files and memory. Finish
or stop active/queued work first. Optional model tests require explicit consent and may incur charges. Browser/memory switches enforce access while preserving saved data; browser
enablement requires a compatible VM. See [tool controls](runtime/agent-tool-controls.md).
See [profile editing](runtime/agent-profile-editing.md) for restart and stale-tab behavior.

Per-agent schedules: open **Agents → Details → Manage schedules** to create
one-time or fixed-interval work, pause/resume it and inspect run outcomes.
The host and Ayati must stay on; scheduled model calls can incur normal charges.
See [schedule controls](runtime/agent-schedules.md) for timing and recovery limits.

The accepted [Floating Canvas design](runtime/design-system.md) styles Home,
Agents and Apps with neutral light/dark colors, raised surfaces and compact
navigation. The [Agents workspace](runtime/agents-workspace.md) has a separate
contact sidebar, floating bottom composer and contextual work panels.
Use **Automation → Schedules** for timed work, **Browser** for the guest view,
and **Details** for settings, memory and files.

### Personal Home

The [Personal Home experience](runtime/personal-home.md) is the default UI: saved
panels, compact agent presence, expandable decisions and an always-available Ayati
guide. Add panel/Create app chooses an agent and opens an unsent chat starter;
requests can also start directly in any conversation. Exact approved publication
can add panels without a second activation step. Installed VMs need the current
guest workspace tools for the new optional publishing fields; older guests retain
the existing preview/review path. See the specification for update and schedule
permissions, browser-local layout preferences and verification limits.

Local PostgreSQL workspace storage and migration: [setup and verification](runtime/postgres-workspaces.md).

For temporary one-assignment workers, set `AYATI_CHAT_EXECUTION=assigned` and run
`ayati assignment-launcher`. Each worker renews its ownership, saves an outcome
and exits. This checkout requires all PostgreSQL migrations through 24; see [assignment workers](runtime/assignment-workers.md)
for configuration, cancellation, recovery and local-only evidence limits.

For prepared shared sessions, set `AYATI_CHAT_EXECUTION=pooled` on the controller
and run `ayati executor` with the private local worker-control connection. Bounded
ready Goose slots receive scoped assignments without core database credentials;
each used session is replaced before reuse. See [the local executor pool](runtime/executor-pool.md)
for startup, readiness, draining and limits. [Live output and reconnect recovery](runtime/reply-streaming.md) are locally
implemented. [Generic harness preparation](runtime/harness-preparation.md) passes
the named local ready-capacity targets; release/live-model calibration and cloud
deployment remain later work. [Chat reserve and background admission](runtime/interactive-reserve.md)
add an explicit deferred-request API and configurable shared capacity headroom.

Shared-agent work can also run in a separate process: set
`AYATI_CHAT_EXECUTION=external` on `ayati identity`, then start `ayati worker`
against the same PostgreSQL database with its own data directory. See
[independent shared-chat workers](runtime/independent-chat-worker.md) for startup,
recovery checks and the conversation-only boundary. Existing embedded execution
remains the default; this increment does not deploy the platform to cloud.

The [hosted deployment foundation](runtime/hosted-deployment.md) adds one container
image for API/workers/migrations, explicit HTTPS origin and Secure cookies,
PostgreSQL sign-in state across API replacement, and readiness checks. The Compose
template is for a private deployment rehearsal; cloud resources are not provisioned.

The [release-image verifier](runtime/release-image-validation.md) builds one
immutable image and checks prepared shared executors, mixed chat/background work,
recovery, signed bridge execution with real prepared Goose and two resource-bounded
load profiles. Run
`python3 tools/verify-release-image.py` with local Docker access. All data and model
responses are disposable/scripted; no AWS resources or paid calls are required.

The [provider calibration workflow](runtime/provider-calibration.md) measures
browser-visible replies using that verified image. Its default rehearsal is
scripted; real-provider execution needs an explicit model/rate/allowance plan and
private key file. It retains request limits, conservative reservations, usage
uncertainty and timing evidence before cloud sizing.
The guide records a first bounded live Fireworks run: seven requests pass, including
concurrent/mixed work and Stop, with the provider key retained outside containers.

The [M0 AWS package](deploy/aws/README.md) adds read-only inventory/pricing tools,
validated inactive CloudFormation definitions and a five-employee/five-agent,
15-minute trial brief. Worker control is still loopback-only: compute and scaling
stay at zero until the M2/M3 startup and machine-identity integration is proven.
See [M0 evidence and limitations](runtime/aws-m0-preflight.md). No AWS resources or
DNS records are created by preparation.

The [packaged signed bridge](runtime/packaged-worker-bridge.md) provides explicit
private-file startup in the same release image. The trial manifest requires
schema 17 for task identity bindings. Its local signed TLS/Goose checks retain
synthetic IAM/ECS/model services; real AWS ingress, identity and lifecycle remain
the next deployment proof.

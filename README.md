# Ayati

Ayati is becoming a local-first business agent workspace powered by Goose, with
Rust control services and a small TypeScript web app. One local agent can now run
inside a real QEMU/KVM Linux VM, with streaming chat, saved sessions, Stop, and
owner decisions for a synthetic MCP tool. The full workspace is still being built;
cloud deployment comes after local stabilization.

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

Open **http://127.0.0.1:8765**. Without provider configuration, the page remains
available and sending is disabled. Restart the daemon after changing configuration.
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

Ayati database schema is now version 3. History and saved Goose mappings are
preserved; legacy queued jobs from earlier schemas become interrupted rather than
being replayed. Older binaries must not open the upgraded database. Automatic
scheduling, idle leases and VM sleep/wake are the next L3 increment.

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
workspace and runtime state. Automatic idle release and scheduled wake are later
work. Starting Ayati can boot the VM for accepted queued work; it never replays
an interrupted prompt automatically.
An unexpected host crash terminates the owned VM; interrupted work needs deliberate
follow-up. Forced termination is reported when clean guest shutdown fails.

The bridge accepts authenticated, fixed operations through SSH with pinned host
keys; it exposes no remote shell or selectable host paths. QEMU has no host-folder
mounts, and restricted networking permits only the private SSH and model gateway
forwards. The guest receives an ephemeral gateway token; the real model API key
stays on the host. General Internet/browser access is not enabled in this proof.
The gateway supports the configured Fireworks/OpenAI chat-compatible provider and
one selected model. Business connector credentials are not implemented yet.

Leaving `AYATI_EXECUTION` unset (or setting `host`) retains the supervised host
mode for adapter development. VM errors never fall back to host execution.
Provider/model/tool/execution changes cannot reuse an incompatible saved session;
there is no automatic host-to-guest session migration. See the
[VM integration record](runtime/local-vm.md) for pins, checks and limits.

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
Provider/model/tool settings are fixed for a saved proof session. To change them,
use a fresh explicit AYATI_DATA_DIR or restore the old settings; the adapter fails
clearly rather than silently loading a session with different tools or a different
provider. Guided reconfiguration is later work.

## Current limits

- One local business/agent/conversation, with one running request and a bounded
  queue. Scoped identities are persisted; multi-user authentication is not built.
- Execution supports one explicitly configured real local VM or the host adapter
  mode. This is not a production multi-tenant service or a complete VM fleet manager.
- Session loading is supported; interrupted prompts and approvals are not replayed
  automatically. Previously queued work is dispatched after a configured restart.
- The prompt has a three-minute limit and text output is bounded to 1 MiB. A
  waiting approval can expire. Failures preserve text and require deliberate follow-up.
- Goose owns working context and private runtime sessions. Ayati owns run records,
  pending decisions and UI history. Long-term sourced memory is not implemented.
- agent-browser, live browser takeover, guided multi-agent setup, connections,
  schedules, generated apps and multimodal processing are planned later slices.
  File input is unavailable on this Goose path until its integration is implemented.
- The service binds to loopback with same-origin guards. It is a local development
  proof, not a production tenant/security boundary. Keep the machine running for work.

There is no provider key in this checkout. Automated Goose checks use real Goose
with a local scripted model; they do not establish live account access, model
quality, or cloud reliability. Never put credentials in chat or workspaces.

## Verification

```sh
cargo fmt --all --check
cargo clippy --workspace --locked --all-targets -- -D warnings
cargo test --workspace --locked
npm test --prefix web
npm run check:e2e --prefix web
npm run build --prefix web
cargo build
node --test --test-concurrency=1 tests/goose_integration.mjs tests/goose_durability.mjs
# Explicit real-VM suite: use a disposable, prepared VM; it modifies its guest state.
AYATI_VM_TEST_MANIFEST=/absolute/path/to/disposable-vm/vm.json node --test --test-concurrency=1 tests/goose_integration.mjs tests/goose_durability.mjs
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

`feat/03-durable-execution` follows the combined published Goose/VM history.
GitHub PRs #10/#11 were merged into the restoration branch after #9 merged into
main; this branch includes that combined tip and current main. Promote the complete
milestones to main when publishing. Original checkpoint branches remain preserved.
Finish, verify and commit each increment before creating the next numbered branch from that completed tip.
Do not develop several independent copies of the same planned files. Once a
parent branch is merged, align the next branch with its merged history before
publishing it; numbering alone does not prevent conflicts. Push/PR only when asked.

Read `AGENTS.md`, `proj-docs/README.md`, the local implementation plan and current
status before the next increment. Planning documents and references remain ignored
and local. Keep each source file at or below 500 lines and keep lockfiles tracked.
SQLite remains the initial Ayati database; hosting alone does not require replacing it.

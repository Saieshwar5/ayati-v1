# Ayati

Ayati is becoming a local-first business agent workspace powered by Goose, with
Rust control services and a small TypeScript web app. This branch implements the
first Goose/ACP slice: one local conversation, streaming, a saved runtime session,
Stop, and owner decisions for a synthetic MCP tool. The full workspace is still
being built; cloud deployment comes after local stabilization.

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
The UI's Stop cancels a request; the CLI's stop shuts down the service and its
owned runtime. Completed external actions cannot be undone by cancellation.

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
- Goose runs as a supervised **host subprocess** in this slice. A private working
  directory is not a sandbox or real VM. Local VM integration is the next stage.
- Session loading is supported; interrupted prompts and approvals are not replayed
  automatically. Starting the daemon does not call the model to resume old work.
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
quality, or cloud/VM reliability. Never put credentials in chat or workspaces.

## Verification

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
npm test --prefix web
npm run check:e2e --prefix web
npm run build --prefix web
cargo build
node --test --test-concurrency=1 tests/goose_integration.mjs
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

Start from `feat/01-goose-local-foundation`. Finish, verify and commit each
increment before creating the next numbered branch from that completed tip.
Do not develop several independent copies of the same planned files. Once a
parent branch is merged, align the next branch with its merged history before
publishing it; numbering alone does not prevent conflicts. Push/PR only when asked.

Read `AGENTS.md`, `proj-docs/README.md`, the local implementation plan and current
status before the next increment. Planning documents and references remain ignored
and local. Keep each source file at or below 500 lines and keep lockfiles tracked.
SQLite remains the initial Ayati database; hosting alone does not require replacing it.

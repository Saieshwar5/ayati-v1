# Cuckoo

Cuckoo is a shared organization workspace with persistent AI teammates. People sign
in to workspaces, talk to agents in Chat, and keep their work in PostgreSQL and
private object storage. Agents run on [pinned upstream Goose](runtime/goose-release.json)
through Rust control services and a TypeScript/React web client
([migration boundary](runtime/react-client.md)).

The product is being prepared for AWS. The old single-user local daemon was removed
in favor of one architecture: the same controller, executor and PostgreSQL shape
runs on a laptop and in the cloud.

The public app name is Cuckoo. Existing `ayati` commands, `AYATI_*` / `RELAY_*`
configuration and database names remain. See [Cuckoo branding](runtime/cuckoo-brand.md).

## Architecture

| Process | Command | Owns |
| --- | --- | --- |
| Controller | `ayati identity` | Sign-in, workspaces, Chat, approvals, schedules, events and all PostgreSQL access |
| Executor | `ayati executor` | Bounded prepared Goose slots that receive scoped assignments from the controller |
| Migration job | `ayati workspace-db migrate` | Explicit PostgreSQL schema changes; never run on API startup |

Executors never receive core database credentials or WorkOS keys. See
[worker control](runtime/worker-control.md), the [shared executor pool](runtime/executor-pool.md)
and [live replies](runtime/reply-streaming.md).

Main product contracts:

- [Shared agent control](runtime/shared-agent-control.md): reviewed remote MCP
  operations with per-conversation Allow/Ask first/Block,
  [explicit approvers](runtime/channel-approvals.md) and durable receipts.
- [Durable approval waits](runtime/durable-approval-waits.md) and
  [questions and forms](runtime/agent-questions.md).
- [Agents workspace](runtime/agents-team-workspace.md),
  [saved reading](runtime/result-reading.md) and
  [chat organization](runtime/chat-organization.md).
- [Home attention](runtime/workspace-attention.md),
  [general Home panels](runtime/general-home-panels.md) and
  [panel maintenance](runtime/home-panel-maintenance.md).
- [Workspace Library](runtime/library.md),
  [shared schedules](runtime/shared-agent-schedules.md) and
  [shared events](runtime/shared-agent-events.md).
- [Versioned conversation context](runtime/versioned-conversation-context.md).

## Run locally

Requirements: Linux x86_64, Rust/Cargo, Node.js 26, PostgreSQL server tools, curl,
tar and sha256sum. The pinned Goose binary is downloaded separately and is not
stored in Git.

```sh
./tools/install-goose.sh
python3 tools/build-prepared-goose.py
npm ci --prefix web
npm run build --prefix web
cp -n .env.example .env          # then edit it locally; never commit secrets
./tools/local-postgres.sh start
./tools/local-postgres.sh migrate
./tools/local-postgres.sh env    # prints the private database URLs
```

Then start one controller and at least one executor. Create the worker-control
bootstrap file as described in [worker control](runtime/worker-control.md#migration-and-local-startup),
and follow [the executor pool guide](runtime/executor-pool.md) for the exact
controller and executor environments. In short:

```sh
AYATI_CHAT_EXECUTION=pooled AYATI_WORKER_PROVIDERS=openai cargo run --locked -- identity
AYATI_GOOSE_PROVIDER=openai AYATI_MODEL=your-supported-model \
  AYATI_DATA_DIR=.ayati-local/executor-one cargo run --locked -- executor
```

Sign-in uses WorkOS (`WORKOS_API_KEY`, `WORKOS_CLIENT_ID`). For a scripted company
without provider accounts, use `ayati simulate <directory>` with a dedicated
simulation database; see [company simulation](runtime/company-simulation.md).

## Deployment

- [Hosted deployment foundation](runtime/hosted-deployment.md): one container image
  for controller, executors and migrations; HTTPS origin, Secure cookies and readiness.
- [Release-image verifier](runtime/release-image-validation.md):
  `python3 tools/verify-release-image.py` with local Docker access.
- [AWS package](deploy/aws/README.md): inactive CloudFormation definitions and
  read-only preflight tools. No AWS resources are created by preparation.
- [Packaged signed bridge](runtime/packaged-worker-bridge.md) for executor identity.

Worker control still accepts loopback connections only. Authenticated remote
executors are the next deployment proof.

## Verification

```sh
cargo fmt --all --check
cargo clippy --workspace --locked --all-targets -- -D warnings
cargo test --workspace --locked
./tools/local-postgres.sh test       # disposable PostgreSQL regressions
npm test --prefix web
npm run build --prefix web
python3 tools/run-agent-load.py --suite workspace   # connected journeys
./scripts/check-file-lines.sh        # local ignored helper, when present
```

Connected journeys use real pinned Goose with a local scripted model. They do not
measure model quality or cloud reliability, and they make no paid model requests.
See [test details](tests/README.md) for the other suites.

## Development

Read `AGENTS.md`, `proj-docs/README.md` and the current implementation status
before an increment. Keep each source file at or below 500 lines, keep lockfiles
tracked, and never put credentials in chat, agent workspaces or Git.

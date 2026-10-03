# Ayati

Ayati is a cloud-first single-assistant product built on OpenClaw. The previous
Rust agent harness and web prototype have been removed from the active tree.
The complete source in `vendor/openclaw` is now the sole agent runtime. It is
tracked here as a squashed Git subtree, not a submodule or ignored reference.

This increment establishes the source/build baseline and manual upgrade policy.
The upstream Control UI is a development interface. Ayati's minimal UI,
customer signup, official channel routing and isolated multi-customer hosting
are still to build. Existing product requirements remain under `proj-docs`.

## Pinned baseline

| Component | Pin |
| --- | --- |
| OpenClaw package | `2026.9.7` |
| Upstream commit | `9d5c37cf80a9bf6c50833c405f0ae5a5b31298a1` |
| Development Node | `26.2.0` (`.node-version`) |
| Package manager | `pnpm 12.5.1` (integrity pin in `package.json`) |
| Dependencies | Tracked upstream `pnpm-lock.yaml`, frozen install |

[openclaw-baseline.json](openclaw-baseline.json) records the source tree and lock
hash. This is the inspected development baseline, not a production release.
The full upstream build needs about 4.7 GB peak memory according to its guard.
Use a build host with sufficient available memory; GitHub CI also builds and
checks the real Gateway and Control UI. The guard remains enabled.

## Build and run

Install the pinned Node and pnpm versions, then run from the repository root:

```sh
pnpm setup:runtime
pnpm build
pnpm check
pnpm test
pnpm test:gateway
pnpm run init
cp .env.example .env
pnpm openclaw configure
pnpm start
```

Configure a model provider through OpenClaw. Optional provider credentials can
live in the ignored `.env`. Archived prototype credentials are not migrated
automatically. Never put real credentials in configuration templates or Git.

The development Gateway listens on `http://127.0.0.1:19889`. Open the dashboard
with `pnpm openclaw dashboard`. Control UI device authentication stays enabled.
The token is generated into private local configuration and is not printed by
our initializer. This is a trusted developer interface, not customer signup.

The launcher uses `.ayati-openclaw` for configuration, credentials, workspace and
runtime state. Set `AYATI_STATE_DIR` for another private directory. Startup runs
a foreground process without installing or managing an OS service. Keep that
Gateway running; Ctrl+C stops it.

## Update ownership

- Automatic Gateway/node updates and runtime update checks are explicitly off.
- The launcher reapplies this policy on startup and sets
  `OPENCLAW_NO_AUTO_UPDATE=1`, even if inherited settings enable updates.
- The agent's `gateway` administration tool and chat config/restart commands are
  disabled. The Ayati launcher refuses the native `update` CLI command.
- Operators upgrade source through Git, review and test it, then build an Ayati
  artifact. Future deployment must pin that artifact by digest.
- The development UI retains trusted operator administration powers. These
  controls do not establish customer authorization or an immutable production
  runtime. Do not expose this baseline as a public service.

See [manual upstream upgrades](docs/upstream-upgrades.md) for fresh-clone setup,
branch/merge commands, validation, release promotion and recovery requirements.

## Repository boundaries

| Path | Owner and purpose |
| --- | --- |
| `vendor/openclaw` | Tracked runtime, tools, plugins and Control UI |
| `bin`, `lib` | Small Ayati launcher and baseline checks |
| `config/openclaw.json` | Public baseline template with no credentials |
| `tests` | State-isolation and managed-update regression checks |
| `docs` | Versioned product development/update instructions |
| `proj-docs` | Existing ignored product requirements and references |

Keep Ayati changes focused. Configure behavior first where sufficient; make
needed runtime/UI changes inside the subtree in distinct commits. Future
subtree merges reconcile our modifications against the imported base. The full
upstream history is not embedded in every Ayati clone. Upstream CI files inside
the subtree do not become active Ayati GitHub workflows.

## Previous prototype recovery

The annotated tag `ayati-before-openclaw-2026-10-03` preserves committed code.
Open it on a separate worktree to retain the new codebase:

```sh
git fetch origin tag ayati-before-openclaw-2026-10-03
git worktree add ../ayati-prototype ayati-before-openclaw-2026-10-03
```

A Git tag does not preserve ignored documents, credentials, runtime data or
build output. Prior source/dependencies, `.env` if present, and a planning
snapshot are separately retained locally under the ignored, private
`.tmp/ayati-before-openclaw-2026-10-03`. Existing `.ayati` data and `target` output
remain untouched; they are not migrated into OpenClaw.

OpenClaw's [MIT license](vendor/openclaw/LICENSE) and copyright notice are retained.
Third-party components keep their respective notices and licenses.

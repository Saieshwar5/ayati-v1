# Generic harness preparation before readiness

The shared executor prepares a fresh, tool-free Goose process before advertising
its slot. An accepted request can then use that process immediately; it does not
wait for a container or first-use tokenizer initialization. Each assigned process
is still destroyed after its one use. Idle agents own no process, but shared ready
capacity consumes compute and replenishment takes time.

## Explicit, pinned preparation contract

Stock Goose 1.54.0 initializes its process-global `o200k_base` tokenizer lazily on
the first context estimate. Its public ACP session setup has no tokenizer warm-up
hook. The separately identified `1.54.0+ayati.1` variant adds one opt-in extension
to `session/new`: `_meta.ayatiPrepare: 1`. It awaits token-counter creation, then
returns `_meta.ayatiPrepared` with `version: 1`, `tokenCounter: "o200k_base"` and
`durationMicros`. Invalid contract versions fail setup. Without the flag, the same
artifact retains lazy initialization, allowing a direct cold/prepared comparison.

This is generic CPU/heap initialization. No user message, tenant context, prompt,
model request or tool is involved. A matching binary version without the complete
acknowledgment cannot supply ready capacity. Initialization has the existing
30-second startup timeout; cancellation drops the owned process group. The
acknowledgment is a compatibility contract from trusted operator software, not a
proof of hostile-executor isolation or business authority.

The small [patch](../tools/patches/goose-1.54.0-preparation.patch) applies to upstream
commit `cd643d2126fffab4c6a6186905211ba868d6a657` (v1.54.0, Apache-2.0).
[The builder](../tools/build-prepared-goose.py) uses its unchanged Cargo.lock,
`--release --locked --no-default-features --features rustls-tls`. It installs the
ELF, license and a manifest recording source, patch, lockfile, compiler and binary
hashes. This lean artifact is intended only for the tool-free pooled profile;
AWS/local-inference/code-mode and other optional features are not enabled. The
ordinary, cold-worker and VM paths retain stock 1.54.0 and `AYATI_GOOSE_BIN`.

Use Python 3.12+, Git and the Rust/C/C++ build toolchain required by the locked
upstream source. The Docker build stage supplies these separately.

~~~sh
python3 tools/build-prepared-goose.py
# Optional existing Git checkout containing the exact pinned commit:
python3 tools/build-prepared-goose.py --source-repo /path/to/goose
~~~

The prepared artifact defaults to `.cache/goose/v1.54.0-ayati.1/goose`; override
with `AYATI_PREPARED_GOOSE_BIN`. The builder verifies an existing matching install
without recompiling. `--rebuild` explicitly replaces the installed artifact after
a successful build. A modified source cache fails rather than deleting local
changes. The Dockerfile builds this variant separately and includes its manifest
and license. First compilation is substantial; it is image/build work, outside
the request path. Native build verification does not establish Docker execution.

## Readiness and upgrades

Run all migrations through `m20261024_000017_cloud_executors` with the migration
owner, then reapply the existing restricted runtime grants. Startup only verifies
the schema. Migration 15 adds a bounded `preparation_version` to the slot registry
and withdraws old unverified ready slots. Both controller reporting and a database
trigger normalize legacy `ready` reports without preparation v1 to `preparing`.
The dispatcher additionally selects version 1. Older executor JSON omitting the
field defaults to version 0; unknown versions are refused.

Migration 15 preserves already offered/running assignments and capabilities.
Deploy the migrated schema and compatible controllers before enabling the new
executors, then drain/replace older executors. Plan temporary extra ready capacity
for that replacement. Old ready sessions cannot serve new work; old assigned
work may finish within its existing lease. This is a forward migration, not an
automatic binary rollback strategy. Follow the existing migration backup,
dispatch-mode and deployment restrictions in [PostgreSQL setup](postgres-workspaces.md)
and [executor pool](executor-pool.md).

Local preparation uses an empty fresh scratch directory and an empty `.env` fence.
The process receives the existing allowlisted provider configuration, no core DB,
WorkOS, bootstrap or slot credentials. Context arrives only after assignment.
No browser, local command/filesystem tool, memory or extension is enabled. Used
scratch/session/process/token/generation are replaced rather than reset for reuse.

## One atomic start and lease confirmation

The controller previously committed start/context ownership and then performed a
second authorized renewal transaction. Start now confirms a full accepted lease
in the same transaction as ownership and the saved context. Exact replay returns
that saved context and refreshes the lease without another attempt. Changed
scope/session, revoked access, expired ownership or a paused agent still fail.

The workspace lock, exact capability check, current chat/agent/source access and
checks around context preparation remain. Later renewals, output and finalization
retain their authorization. This removes a redundant transaction without caching
permission decisions or treating memory/model output as authority.

## Verification and limits

The [bounded runner](bounded-agent-load.md) adds `--suite preparation`. It compares
eight fresh processes using the same custom ELF with and without preparation,
checks zero pre-ready inference/host requests, rejects a version-matching process
without the acknowledgment, and drains stopped initialization children. All
provider traffic goes to the local scripted fixture. Separate PostgreSQL tests
cover legacy reports, migration from version 14, retained owned capabilities,
exact replay and denied lease extension.

The direct same-ELF probe passed all eight trials. Session-only prompt-to-model
arrival was p50/p95 156/231 ms; prepared was 19/21 ms. Session setup moved from
133/160 ms to 279/288 ms. This is four observations per mode, a small integration
probe rather than a tail-latency benchmark. All three preparation journeys passed,
including absent-acknowledgment refusal and cancellation/reaping during blocked
initialization. Evidence: `.tmp/bounded-agent-c04mscub` and
`.tmp/goose-preparation-P5QIIp`. The final broad matrices use an immutable Ayati
binary; the earlier protocol/process-lookup test failures remain retained in
`.tmp/bounded-agent-g6arn4tw`.

The real native release build succeeded, with manifest/binary verification and
149 default Rust tests, 78 disposable PostgreSQL checks, four Python resource
boundary tests (also with `python -O`), strict all-target Clippy and debug build.
The Docker image build remains unverified: this session cannot access the Docker
daemon and passwordless sudo is unavailable. No ordinary database was migrated,
and no AWS resource or paid model was used.

Ready dispatch, model-request arrival, first token and final reply are separate
measurements. Shared ready capacity cannot guarantee model latency or instant
service after saturation. See the bounded report for measured results and open
targets; cloud networking, service scaling, provider identity and live models
remain separate acceptance work.

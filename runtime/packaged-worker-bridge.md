# Packaged signed worker transport

The release image includes the optional Python bridge, its locked SDK/HTTP
dependencies and the actual pinned prepared Goose. Run `bridge /absolute/config.json`
through the image's normal entrypoint. Ordinary API, migration and executor commands
retain their existing behavior. This is a local packaging/transport increment;
the AWS templates remain inactive and the CLI does not provision resources.

## Startup and private configuration

The bridge is a separately supervised process. Both modes bind **127.0.0.1 only**;
there is no configurable public bind, local-signing bypass or fixture option.
The private JSON file must be absolute, owner-owned, regular, not a symlink,
0600 and at most 16 KiB. Unknown fields, inline secrets, invalid ports, wrong
infrastructure identities and non-loopback backend destinations fail startup.
Diagnostics include only a stage and exception class, never configuration values.

Use the [worker example](../integrations/aws_bridge/worker.example.json) and
[controller example](../integrations/aws_bridge/controller.example.json) as shapes,
not deployment inputs. Supply the actual regional gateway, account, task role,
cluster and immutable task-definition revision from reviewed infrastructure.

For development outside the image, invoke the isolated interpreter with the module:

```sh
PYTHONPATH="$PWD/integrations" .tmp/aws-bridge-tools/bin/python \
  -m aws_bridge /absolute/private/bridge.json
```

For the image, the container command is:

```text
bridge /run/secrets/ayati/bridge.json
```

The image runs as UID/GID 10001. A trusted supervisor supplies readable configuration
and secrets owned by that UID, an isolated writable task directory, a read-only
root filesystem, dropped capabilities, no-new-privileges and bounded resources.
No package installation or compilation occurs at container startup.

## Worker mode

Worker mode creates two distinct random local capabilities in the configured
files. Their existing parent directories must be owner-owned 0700, without
symlinks; files are checked as owner-owned 0600 regular files. Control and model
ports, paths and key values must differ. Existing keys can be reused only within
the same task-local directory; give every replacement task fresh scratch.

The supervisor starts `executor` separately, in the same network namespace, with:

| Executor setting | Source |
| --- | --- |
| `AYATI_WORKER_CONTROL_URL` | `http://127.0.0.1:<control_port>` |
| `AYATI_WORKER_CONTROL_KEY_FILE` | Worker `control_key_file`, never the controller bootstrap |
| `OPENAI_HOST` | `http://127.0.0.1:<model_port>` |
| `OPENAI_API_KEY_FILE` | Worker `model_key_file`, loaded by the existing entrypoint |
| `AYATI_GOOSE_PROVIDER` | `openai`, the tested compatibility path |
| `AYATI_MODEL` | The explicitly tested model |

Only the signing bridge receives ambient task-role credentials. It needs no core
database, WorkOS or upstream provider credential. CLI startup rejects those values
and their `_FILE` forms. The executor and its Goose children retain their existing
credential filtering. This does not establish container-level credential
separation for a future ECS task; that topology still needs an actual cloud proof.

The bridge is not an executor supervisor and does not claim assignment readiness.
Start the executor only after both local listeners answer `/health` with 204;
admit work only after the controller observes prepared executor slots. A local
health response proves neither gateway access nor model availability. Restart or
drain coupled processes through the deployment supervisor; never activate a new
task by copying another task's bootstrap/configuration.

## Controller mode

Controller mode reads the existing private operator bootstrap and a separate
private model-proxy reference: `{"url":"http://127.0.0.1:<port>","key":"<local capability>"}`.
The model capability is 43 URL-safe characters and must differ from the bootstrap.
It references an independently bounded local proxy, not a raw provider key.
Provider/billing policy and proxy delivery remain separate deployment work.

The controller bridge must share a network namespace with Rust's private control
listener. Its principal header is trusted only from a separately authenticated
front door that overwrites it. The loopback restriction is intentional: this
increment does not make that front door, private ingress or AWS identity available.
Never expose the controller bridge or Rust port directly to an untrusted network.

All controllers require migration **17**, runtime grants and the existing cloud
identity configuration. The trial manifest now requires 17 as well; old schema-16
manifests fail review rather than starting a release without identity bindings.

SIGTERM/SIGINT stop accepting requests and close both listeners/connections. A
failed second bind closes the first listener. Assignment Stop, renewable leases,
task binding, cancellation and stale-attempt rejection remain Rust-owned.

## Reproducible local acceptance

```sh
# Verify/build the actual prepared artifact and native service, then run privately.
python3 tools/build-prepared-goose.py --jobs 1
cargo build --locked --bin ayati -j 1
AYATI_TEST_BRIDGE_PYTHON="$PWD/.tmp/aws-bridge-tools/bin/python" \
  python3 tools/run-agent-load.py --suite bridge --executor-cpu 200

# Build one immutable image and run image/mixed/recovery plus signed transport.
python3 tools/verify-release-image.py --smoke-only --build-jobs 1
```

Use the [adapter setup](../integrations/aws_bridge/README.md#local-checks) to prepare
the isolated interpreter first. Unset `AYATI_TEST_PREPARED_GOOSE_BIN`; the signed
journey must report actual pinned prepared Goose. The image path executes the
bundled bridge/startup code, controller and Goose from the same immutable image.
Only the synthetic gateway verifier and its TLS socket mapping come from mounted
test fixtures. They never enter the image or become production configuration.

The journey covers five private employee/agent identities, two single-slot
executors, queued work, no inference before readiness, private source separation,
controller replacement, Stop, process-loss recovery, stale credentials and wrong-
task/direct-backend denial. Image startup checks additionally execute the real
entrypoint under UID 10001, read-only root and resource ceilings.

The provider, IAM gateway and ECS inventory remain scripted. This acceptance does
not establish live AWS authorization, real-model latency, scaling or teardown.
Continue with M2/M3's gateway/startup/storage and independently enforced cleanup
before the bounded private AWS trial.

## Verified local checkpoint — 2026-10-25

Built Linux amd64 image:
`sha256:443eb7c3e6ea950229dc56c0f33d8c3d4e0714beb28622fb5c653abb01972d80`.
Build provenance and initial image checks are retained in
`.tmp/release-image-xsmvx6sk`; the initial bridge failure remains recorded there.
The restricted fixture ownership helper was corrected to take directory ownership
before enumerating its private files, retaining the same capabilities.

Actual host prepared-Goose transport passes both journeys in
`.tmp/bounded-agent-qyc5wgny`. The same journeys then pass with the bundled
controller, bridge/startup code and prepared Goose in
`.tmp/bounded-agent-7nmlcfma`. Packaged startup also passes all nine startup,
shutdown, partial-failure, permission and credential-denial tests through the
normal image entrypoint. The first image suite passes all eleven existing
image/mixed/recovery journeys, including 96 mixed-workload chats.

All 42 focused Python checks pass normally and with optimization. Native Ayati
build, pinned host artifact verification, source syntax, local link checks and
the source line-limit check pass. These results use fresh private PostgreSQL
clusters and synthetic companies/providers; ordinary workspace data, AWS
resources, DNS and paid inference remain untouched.

Final combined `--image ... --smoke-only` verification passes in
`.tmp/release-image-3v9w4hdd/verification.json`: eleven image/mixed/recovery journeys
and two signed bridge journeys, all using that same immutable image. It points to
`.tmp/bounded-agent-48gwtlyq` and `.tmp/bounded-agent-4yf5mely`. The two ordinary
384-request matrices were deliberately not repeated for this packaging change.

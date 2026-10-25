# Controller/worker transport adapter

This optional Python adapter keeps Rust's machine API on loopback while adding a
signed HTTPS hop. Its acceptance harness runs entirely on the development host.
It does not provision infrastructure or establish that an AWS gateway is configured.

The controller still owns database access, authorization, durable output and task
ownership. Workers receive private local bootstrap files and per-assignment
capabilities, without the core database, WorkOS or real provider credentials.

## Identity and request boundaries

The worker bridge signs allowed routes with the maintained AWS SDK. The controller
bridge expects a principal header overwritten by a trusted IAM front door. At
registration, `TaskVerifier` checks the task's cluster, running state, exact task
definition and role through ECS. Caller-supplied task metadata is rejected.

Rust's private `/v1/executors/register-cloud` route additionally requires the
operator bootstrap, `AYATI_CLOUD_WORKERS=1`, pooled execution and these configured
identities:

- `AYATI_AWS_EXECUTOR_CLUSTER_ARN`
- `AYATI_AWS_EXECUTOR_RELEASE_ARN`
- `AYATI_AWS_EXECUTOR_ROLE_ARN`

Migration 17 (`m20261024_000017_cloud_executors`) makes the registration boundary
immutable and binds one executor/incarnation to one task. A cloud capability must
arrive with that task's trusted identity on every control request. Local and cloud
registrations cannot convert one another, including concurrent registration.
Exact replays preserve the binding; wrong identity, reused credential or duplicate
task registrations are denied and rolled back. Existing local executors and
assignment credentials retain local behavior after migration.

Workers forward only explicit method/path combinations. The bridge rejects browser
credentials, ambiguous authorization/framing headers and oversized request bodies.
Local destinations require an explicit `127.0.0.1` HTTP origin. Other transport
destinations require TLS verification. Private configuration uses owner checks,
0600 files, bounded reads and no-follow opens. Responses stream incrementally;
unfinished responses close upstream connections. Each server bounds request
threads and socket inactivity. Unexpected failures log only their stage and class,
without credentials, request paths, payloads or provider bodies.

The HTTP transport does not retry or follow redirects. Existing Rust capability
replay, lease, cancellation and stale-attempt rules remain authoritative.

## Dependency choice

`requirements.lock` records boto3/botocore 1.43.118 and urllib3 2.8.0 plus their
resolved dependencies. The SDK supplies credential resolution, SigV4 and ECS
clients; urllib3 supplies pooled, certificate-verified incremental HTTP. This keeps
AWS-specific integration outside the Rust core. No custom signing implementation
is used in the runtime. Startup refuses urllib3 versions without `read1`.
Boto3/botocore use Apache-2.0 licensing; urllib3 uses MIT licensing.

The local gateway fixture uses real SDK signatures and a generated, verified TLS
certificate with synthetic credentials. It overwrites identity headers and sends
all traffic to loopback. Its signature verifier and task inventory are test
substitutes for AWS; they cannot establish actual IAM or ECS authorization.

## Local checks

Install into an isolated environment and invoke its executable directly:

```sh
python3 -m venv .tmp/aws-bridge-tools
.tmp/aws-bridge-tools/bin/python -m pip install -r integrations/aws_bridge/requirements.lock
.tmp/aws-bridge-tools/bin/python -m unittest discover -s tests -p 'aws_bridge*_test.py'
.tmp/aws-bridge-tools/bin/python -O -m unittest discover -s tests -p 'aws_bridge*_test.py'
```

Do not resolve the interpreter symlink to the system executable: that bypasses
the virtual environment and can select incompatible system packages.

With a dedicated local `RELAY_TEST_DATABASE_URL` named `relay_simulation_*`,
PostgreSQL client/server tools and OpenSSL, run the protocol rehearsal:

```sh
cargo build --locked --bin ayati -j 1
export AYATI_TEST_BRIDGE_PYTHON="$PWD/.tmp/aws-bridge-tools/bin/python"
export AYATI_TEST_PREPARED_GOOSE_BIN="$PWD/tests/fixtures/bridge_runtime.mjs"
node --test --test-concurrency=1 tests/aws_worker_bridge.mjs
```

This uses the real Rust controller and executors, real TLS/SDK signing, a scripted
ACP runtime and an authenticated scripted model. The ACP fixture explicitly
identifies itself in the report. Its readiness acknowledgment is simulated; it
establishes neither actual Goose preparation nor latency/release acceptance.
Five active employee identities and five explicitly shared agent profiles use two
single-slot executors. Checks cover queueing, private context, credentials,
controller replacement, Stop and process-loss recovery. The common company fixture
seeds a larger roster; this journey actively uses five employee identities.
Captured synthetic capabilities stay in memory and are omitted from reports.

Separately, `tests/worker_control.mjs` runs the actual pinned upstream Goose against
the existing local cold-worker API, covering restart, cancellation, lease loss and
lost acknowledgments. This preserves the existing execution path; it does not
substitute for a real prepared-Goose run through the new bridge.

For that later acceptance run, build the prepared artifact sequentially with
`python3 tools/build-prepared-goose.py --jobs 1`, unset
`AYATI_TEST_PREPARED_GOOSE_BIN`, and rerun the process suite. The full prepared
build did not complete on the small development host. Do not put the scripted
fixture into a deployment or use it for performance, cost or readiness evidence.

No paid model or AWS call is needed for these checks. Existing local assignment,
pool and database suites remain the regression checks for the underlying protocol.

## Deferred cloud work

Actual AWS IAM verification, private ingress/header-overwrite rules, streaming
gateway configuration, image packaging, secret delivery, storage, Fargate lifecycle
and independently enforced resource teardown remain unverified. A process timer
or model-request window does not remove cloud infrastructure. The interrupted
cloud startup launcher is not part of this supported library checkpoint.

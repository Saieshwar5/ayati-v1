# M0 AWS private-trial package

This directory prepares the selected EC2 controller / shared Fargate executor /
RDS PostgreSQL / private S3 deployment. It does not deploy it. The CLI only makes
allowlisted reads, validates local files and writes private evidence.

Owner's scenario: **5 employees, 5 agents, 15 minutes of active cloud testing**,
with low spending. The owner has `cuckoo.onl`; `trial.cuckoo.onl` is the proposed
trial hostname. DNS has not been changed. A numeric total budget and exact UTC
cleanup deadline remain unset rather than guessed.

## Current activation boundary

The Rust worker-control implementation accepts **loopback HTTP only** and uses
a private local bootstrap file. The [packaged signed adapter](../../runtime/packaged-worker-bridge.md)
adds local transport acceptance; actual AWS gateway/task authentication and startup
remain unproven. Preserve that boundary; do not publish port 8766, copy one shared
bootstrap secret across tasks, or replace the pooled executor with the old
database-connected polling worker.

Consequently the templates enforce zero controller instances, zero executor
tasks and zero suspended scaling capacity. The controller launch script exits
with an integration-required message. The executor has no cloud bootstrap Allow
policy or populated local bootstrap file. These are reviewable **inactive
definitions**, not a working cloud service. The CLI always reports
`deployment_ready: false`, even with every input supplied.

Creating even these inactive stacks would still create billable RDS, NAT, ALB,
storage and other resources. Preparation and template validation create none.
M2/M3 must replace the activation gate with tested startup, authenticated machine
transport, exact task/incarnation/slot binding, Fargate scratch, drain/protection
and actual demand metrics before live context or tasks.

## Resource and dependency order

All stacks require the same `Environment` and `ExpectedAccountId`. Every stack
rejects another AWS account or region; this package is limited to `ap-south-1`.
Stack exports use the environment prefix. Create separate prefixes for separate
trials. Review the manifest's resource inventory and parameter map together.

| Order/template | Defined resources and deliberate limits |
| --- | --- |
| `network.yaml` | Dedicated 10.48.0.0/16 VPC; public, application and isolated database subnets in two distinct zones; one zonal NAT; S3 gateway endpoint including ECR layer reads; HTTPS-only public ingress; no executor ingress or worker-control exposure |
| `data.yaml` | Private encrypted Single-AZ db.t4g.small PostgreSQL, pinned engine, 20 GiB gp3, TLS required, seven-day backups/log retention, deletion protection and snapshot retention; private versioned retained S3 originals with no automatic object expiry |
| `operations.yaml` | Immutable ECR tags and scan-on-push; retained controller/executor log groups with seven-day event retention |
| `permissions.yaml` | Separate bounded controller, migration, ECS execution and executor roles; external reviewed permissions boundary; exact secret references; an unattached, narrowly scoped PassRole policy component |
| `controller.yaml` | Explicit inspected x86_64 AMI, t3.medium with standard credits, IMDSv2, encrypted replaceable root, no public IP; ASG min/max/desired **0** and a guarded startup stub |
| `executors.yaml` | Shared Linux amd64 Fargate service, 2 vCPU / 4 GiB task, two prepared slots; non-root/read-only harness, private ephemeral volumes; desired and scaling min/max **0**, all scaling suspended |
| `edge.yaml` | Public HTTPS ALB with existing issued regional ACM certificate, exact-host routing, replay-compatible timeout candidate; unknown hosts return 404; outputs DNS target without changing DNS |

The initial trial is not highly available: one controller/database/NAT is a single
point of failure. Fixed log retention is not notification delivery; alarms,
aggregate demand publication and alert recipients still need the M2 integration.
Task scale protection is not implemented by the current executor; there is no
CPU-only or pretend demand scaling policy in this revision.

## Candidate active profile, after M2/M3

The manifest prices one controller and **two** two-slot executors, four slots
total. Global/workspace/model admission limits are 4 with chat reserve 1 and a
180-second per-attempt bound. Five agents are identities; five simultaneous runs
need not be active. The fifth queues. Background work can use at most three of
four compatible ready slots while the reserve protects a chat offer.

This matches the recorded four-slot, two-CPU local image profile and enables a
two-task machine-identity proof. It is a candidate for this small trial, not
production sizing. Local trials bounded each executor at 1 GiB; the 2-vCPU
Fargate shape requires at least 4 GiB. Actual Fargate memory/startup still needs
measurement. The price calculation includes both whole tasks, not just active
model-generation time. Service maximum is not a hard spending cap during rollout.

Fargate bind volumes start root-owned. The proposed trusted, secret-free init
container changes volume permissions before the UID 10001 harness starts. It
drops default capabilities except CHOWN; the harness drops ALL. This arrangement
passes schema review but still requires an actual Fargate ownership/capability
check. Its scratch is temporary, not permanent agent files or browser profiles.

## Install validation tools

Use Python 3.12+ with pip/venv and AWS CLI v2. Keep tools outside the application:

```sh
python3 -m venv .venv/aws-preflight
.venv/aws-preflight/bin/python -m pip install -r deploy/aws/requirements.txt
```

`requirements.lock` records the tested tooling resolution. cfn-lint 1.58.0
(MIT-0) supplies maintained CloudFormation schemas; PyYAML 6.0.3 (MIT) reads
long-form intrinsic YAML. No application/runtime dependency was added. Local
schema checking, a malformed-template rejection, and actual AWS ValidateTemplate
are separate checks. They do not establish runtime behavior or create permission.

## Run preparation

Run from the repository root. Reports are owner-readable files and refuse
overwrites/symlinks. Use new output names for repeated runs. The audit/prices
commands need only AWS CLI; review/templates/tests also need the validation venv.
Do not use fixture credentials or print secret values.

```sh
python3 tools/aws-preflight.py audit --output .tmp/aws-trial/audit.json
python3 tools/aws-preflight.py prices --output .tmp/aws-trial/prices.json
python3 tools/aws-preflight.py estimate --prices .tmp/aws-trial/prices.json \
  --resource-hours 1 --output .tmp/aws-trial/candidate-cost.json
.venv/aws-preflight/bin/cfn-lint --regions ap-south-1 -t deploy/aws/*.yaml
.venv/aws-preflight/bin/python tools/aws-preflight.py templates --aws \
  --output .tmp/aws-trial/template-validation.json
.venv/aws-preflight/bin/python tools/aws-preflight.py review \
  --manifest deploy/aws/trial.example.json --allow-incomplete \
  --output .tmp/aws-trial/review.json
.venv/aws-preflight/bin/python -m unittest discover -s tests -p 'aws_*_test.py'
```

Copy the example into private ignored storage before entering account-specific
inputs. It is intentionally incomplete. The review emits exact resource types,
stack parameters/tags, missing inputs, controller settings and activation gates.
Without `--allow-incomplete`, missing inputs fail. That flag allows an incomplete
**review**, never activation or invalid values. Unknown fields, raw-key fields,
wrong-account secret/image/certificate references, mutable image tags, reused
credentials, bad expiry/budget and expanded capacity/duration fail validation.

`--resource-hours` is the complete running window to estimate, including startup
and cleanup. It is not the active test timer. NAT/ALB estimates include at least
one billed hour. Rates must be current Mumbai USD rates no older than 24 hours;
missing, ambiguous, tiered or wrong-unit rates do not become zero-cost successes.

## Inputs before provisioning

The example fixes only the scenario and candidate shape. Fill and verify:

- Intended account, opaque operator/owner tag, source commit and explicit AMI.
- Approved model/provider route and role-scoped secret ARNs; no secret contents.
- **Registry manifest digest** after authorized publish. The previously tested
  local Docker image ID is not an ECR manifest digest.
- Issued Mumbai ACM certificate covering the hostname; matching WorkOS callback
  `https://trial.cuckoo.onl/auth/callback` and client environment. Confirm the
  subdomain before changing the authoritative DNS provider.
- Reviewed deployment/operator identity and pre-existing runtime boundary.
  The PassRole component covers only the named controller and ECS roles, with
  `iam:PassedToService`; it is not a complete or automatically attached deploy policy.
- Distinct administrator and restricted runtime PostgreSQL secrets. The data
  stack produces the RDS-managed administrator secret ARN; an explicit migration
  operation must initialize the runtime role/grants and its separate secret.
- Numeric total budget and UTC expiry within 24 hours. The owner's low-cost
  preference is not translated into an invented approved dollar limit.

The candidate controller settings use `AYATI_CHAT_EXECUTION=pooled`, not the older
Compose polling mode. M2 must deliver private mounted secret files, verified RDS
hostname/CA TLS, WorkOS and scoped storage setup. The existing entrypoint reads
files but is not a Secrets Manager client. Do not copy the administrator DB URL
to the controller or any DB/WorkOS secret to an executor.

## Deployment and recovery handoff

This is the ordered M2/M3 procedure to implement and test, not an executable
deployment command in the M0 CLI:

1. Refresh identity/permissions, quotas, orderability and prices; complete the
   private manifest and review every billable resource and retention policy.
2. Prepare the authenticated cloud-control and startup adapters and their denial
   tests. Review template changes that enable compute, scaling and task protection.
3. Publish the already verified image by immutable registry digest. Validate
   revised templates and inspect change sets, named IAM capabilities and tags.
4. Create stacks in dependency order. Establish the restricted DB role; perform
   one serialized forward migration through schema 18 and reapply runtime grants.
5. Admit a controller only after TLS, private storage and authentication checks.
   Admit executors only after exact machine identity, clean scratch and readiness.
6. Perform the bounded journey in [trial-runbook.md](trial-runbook.md). Record
   identity, digest, start/stop/cleanup times and retained resource inventory.

Code rollback selects a compatible previous image and pauses new admission when
needed. Schema/data restore is a separate operation: take consistent backups,
restore into isolation, validate access/counts/checksums and fence old execution
before dispatch. No automatic down-migration or external-action replay.

Teardown first disables new admission and all scaling/replacement, then drains
and stops executors/controllers. Delete edge/network and disposable resources
only after dependency checks. RDS deletion protection requires an explicit
reviewed change; its final snapshot, S3 versions, registry, secrets and logs can
remain billed. Record and resolve each retained resource deliberately. An expiry
tag is bookkeeping, not a working automatic cleanup timer.

See [recorded M0 evidence](../../runtime/aws-m0-preflight.md) for actual results.
Primary references: [cfn-lint](https://github.com/aws-cloudformation/cfn-lint),
[AWS template validation](https://docs.aws.amazon.com/AWSCloudFormation/latest/UserGuide/using-cfn-validate-template.html),
[Fargate bind volumes](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/bind-mounts.html),
[AWS pricing](https://docs.aws.amazon.com/awsaccountbilling/latest/aboutv2/price-changes.html).

# AWS M0 preflight and inactive infrastructure

Implemented on `feat/aws-m0-preflight`, based on `50519d98`. This is a preparation
increment, not an AWS deployment. [Package and commands](../deploy/aws/README.md),
[trial brief](../deploy/aws/trial-runbook.md),
[incomplete public example](../deploy/aws/trial.example.json).

## Owner scenario and actual boundary

The owner selected a small company with **five employees and five agents**, low
spending and **15 minutes of active cloud testing**. The owner has `cuckoo.onl`;
`trial.cuckoo.onl` is a proposed trial address. DNS, WorkOS configuration, account
permissions, ordinary databases and cloud resources were not changed. No model
calls were made. A numeric budget and actual cleanup deadline remain unset.

Current worker control deliberately accepts loopback HTTP and a private local
bootstrap file. The prepared executor cannot communicate securely across AWS
tasks yet. This increment preserves that boundary instead of exposing the local
listener, copying a shared bootstrap credential or restoring direct-PG workers.

All seven CloudFormation stacks have target-account/Mumbai guards. Controller
ASG min/max/desired, executor desired count and scaling min/max are zero; dynamic
and scheduled scaling are suspended. Controller UserData is an explicit failing
M2 startup stub. The executor task has no cloud bootstrap Allow permission and
no fabricated local key. Even a complete manifest reports `deployment_ready:
false`. Creating inactive stacks would still incur data/network/edge charges.

## Implemented code and invariants

`tools/aws-preflight.py` has only audit, prices, estimate, review and templates
commands. Its AWS runner rejects operations outside a read-only allowlist before
starting a subprocess. Reads use bounded timeouts/attempts and no secret-value
APIs. AWS denial bodies are not copied into reports. Evidence uses exclusive
owner-readable files, preserving earlier attempts and refusing output symlinks.

The manifest rejects unknown fields/inline-key fields, wrong-account or wrong-
region references, mutable image tags, reused administrator/runtime credentials,
invalid budget/expiry, capacity expansion and active compute. The review lists
each resource, stack inputs/tags, missing values and the controller's future
startup contract. `--allow-incomplete` permits a review with missing inputs; it
does not accept invalid values or enable deployment.

Templates define a dedicated two-zone VPC with single-zone NAT, private encrypted
RDS and retained versioned S3, immutable ECR, bounded retained logs, separate IAM
roles and exact service-constrained PassRole component, HTTPS exact-host routing,
and dormant controller/executor resources. Runtime permissions boundaries must
already exist and be reviewed. PassRole is deliberately unattached, not a complete
deploy policy or proof the current identity can create roles/resources.

Candidate active sizing: one t3.medium controller, one db.t4g.small PostgreSQL 18.6
with 20 GiB gp3, and two Linux x86_64 Fargate tasks, each 2 vCPU/4 GiB/two slots.
Global/workspace/model admission is four with reserve one. A fifth concurrent
agent request queues. Two tasks support the later cross-task identity-denial
proof. This retains the recorded local two-CPU/four-slot profile; Fargate's
minimum memory for the chosen CPU shape exceeds the local 1-GiB executor ceiling.
It is not production capacity or cloud latency acceptance.

The proposed secret-free root init container prepares Fargate bind-volume ownership
before the UID 10001, read-only, capability-dropped executor. Its only retained
default capability is CHOWN. This passes schema checks; actual Fargate volume and
capability behavior remains a required M2 proof. Scratch is not permanent storage.

## Read-only AWS findings — 2026-10-24

AWS CLI **2.38.5** authenticates through the **ayati-dev-agent** assumed role.
Private account/ARN/AMI details are retained only in ignored evidence.

Selected Mumbai inventory found one default VPC and no EC2 instances, RDS
instances, ECS clusters, ECR repositories, NATs, Elastic IP allocations, load
balancers, CloudFormation stacks, ACM certificates, secret metadata or log groups.
Account bucket and hosted-zone lists were also empty. This is scoped discovery,
not a worldwide/account billing audit; the development machine may be elsewhere.

Service discovery, public AMI parameter lookup, engine orderability and selected
quotas succeed. The Amazon-owned candidate AMI is available, x86_64, with the
expected root device. PostgreSQL **18.6** on **db.t4g.small/gp3** is orderable.
The observed regional On-Demand Fargate quota is **30 vCPU**; the standard EC2
On-Demand quota is **32 vCPU**. Quotas do not reserve capacity or guarantee launch.
Actual RDS SQL/TLS/restore compatibility is untested.

`iam:GetRole`, `iam:ListAttachedRolePolicies` and `iam:ListRolePolicies` are denied.
Successful discovery/ValidateTemplate does not establish create/update/PassRole
permission. The initial broad EC2-quota listing timed out; the targeted standard
vCPU quota read succeeds and is used in the final reusable audit.

## Cost evidence

The current AWS Price List Query API returned all twelve selected Mumbai USD
rates, captured **2026-10-24** with SKU/effective-date evidence. The initial RDS
query returned both unused-reservation and running-instance products; the tool
rejected the ambiguity. Adding the exact running-instance usage type yields one
correct rate. Historical/failed query evidence is retained.

Under the explicitly priced quantities, the candidate subtotal is **$0.419459
for one resource-hour**, allowing more resource time than the 15 active minutes.
An illustrative 0.25-resource-hour subtotal is **$0.181290**, including at least
one billed NAT/ALB hour. That shorter window is not a realistic promise of full
startup/test/cleanup completion.

These are **subtotals**, not final costs or approved ceilings. Quantities include
two full executor tasks, one controller/database, their disks, NAT with 0.25 GB
processing, one ALB/LCU, at least three public IPv4 addresses and four secrets.
Monthly storage/secret rates use a 730-hour proration assumption.

Excluded/unbounded: model calls, RDS surplus CPU credits, Internet/inter-zone transfer, S3/ECR storage and
requests, logs/secret API calls/DNS, retained backups/objects/secrets after cleanup,
extra scaling/rollout overlap, delay and tax. Every estimate carries these limits
and `hard_spend_cap: false`. Missing/ambiguous/tiered/wrong-unit rates or stale,
non-USD/non-Mumbai catalogs cannot be presented as complete rate sets.

## Verification and retained evidence

Passed: **21 focused Python tests**, local cfn-lint **1.58.0** against Mumbai,
and actual AWS **ValidateTemplate for all seven stacks**. Configuration tests cover
wrong identity/resource references, raw-key rejection, duplicate credential use,
expired/unbounded trial settings and accidental compute activation. Template
boundaries cover core-secret denial, role/PassRole limits, public storage/DB and
worker-control exposure. Price tests cover ambiguity, denial, missing rates,
wrong units/region/freshness and non-finite windows. The AWS runner rejects a
mutating operation before subprocess invocation.

The same 21 tests pass with Python optimization enabled. All **533 source files**
pass the repository line-limit check; new templates and guides are below 500 lines.
CLI review rejects incomplete configuration without `--allow-incomplete` and
rejects inline credential fields without disclosing their contents. Local link,
syntax and Git whitespace checks pass.

Validation tooling is isolated in a private local venv; the dependency lock is
tracked. cfn-lint 1.58.0 (MIT-0) and PyYAML 6.0.3 (MIT) reuse maintained schema/YAML
handling rather than an Ayati template parser. A deliberately invalid template
is rejected by cfn-lint. These checks do not execute a Fargate task or the EC2 stub.
No Rust/client/schema change requires rebuilding the application in this increment.

Evidence root: `.tmp/aws-m0/`. It contains the initial and final inventory,
AMI inspection, pricing attempts/final rates, one-hour/15-minute subtotal reports,
private populated review manifest, resource review, AWS template responses and
final source-hash/check audit. The private manifest fills observed account/zones/
AMI/source commit while retaining absent budget, expiry, secret/certificate/role
references and registry digest explicitly as missing.

## Remaining work

M0 provides the inactive review package and reproducible evidence, with unresolved
deployment inputs. M2/M3 must implement and prove controller startup/private
secret delivery, restricted runtime DB grants/verified TLS, actual S3 storage,
the authenticated cloud-control adapter, exact machine/session binding,
Fargate scratch/readiness, protected drain and demand-based scaling. The
15-minute cloud trial has **not** run. A timer/tag is not an automatic cleanup
system, and a maximum ECS desired count is not a hard billing cap.

Before any provisioning, review the concrete resource manifest and full spending
allowance/expiry, settle the model route, inspect actual deploy-role/boundary
permissions, publish/pin the registry digest and arrange certificate/DNS/WorkOS.
Before customers, complete production isolation, backup/restore, multi-controller
operation and longer failure/load/provider acceptance.

Primary sources: [AWS pricing API](https://docs.aws.amazon.com/awsaccountbilling/latest/aboutv2/using-price-list-query-api.html),
[Fargate task sizes](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/task-cpu-memory-error.html),
[bind volumes](https://docs.aws.amazon.com/AmazonECS/latest/developerguide/bind-mounts.html),
[scaling suspension](https://docs.aws.amazon.com/AWSCloudFormation/latest/TemplateReference/aws-resource-applicationautoscaling-scalabletarget.html),
[cfn-lint](https://github.com/aws-cloudformation/cfn-lint).

# Ayati on AWS: the test cycle

Ayati is built, run and tested on AWS in short windows: create an environment,
test, save the evidence, delete everything temporary, improve locally, repeat.
Controllers and executors are separate ECS Fargate services sharing one image
(see [hosted deployment](../../runtime/hosted-deployment.md)).

```
PERMANENT (stack ayati-foundation, about $2/month)
  ECR repository ayati/runtime      budget alerts (email)
  Route 53 zone dev.cuckoo.onl      wildcard certificate
  secrets ayati/workos-api-key, ayati/model-api-key,
          ayati/worker-control-server, ayati/worker-control-ca

KEPT BETWEEN WINDOWS (stack ayati-dev-keep and snapshots; removed by `forget --yes`)
  keep      files bucket, database passwords
  snapshots ayati-dev-keep-<time>: the database at the end of each window (newest two)

TEMPORARY (stacks ayati-dev-*, created by `up`, deleted by `down`)
  network   VPC, two zones, one NAT, ayati.internal service discovery
  data      Aurora PostgreSQL Serverless v2 (restored from the latest snapshot)
  roles     execution and task roles inside ayati-runtime-boundary
  services  ECS cluster: controller (ALB, app.dev.cuckoo.onl), executor,
            one-off setup task (migrate, then grant the runtime role)
```

Data carries over between windows: `down` stops the services, snapshots the
database and keeps the files bucket, and the next `up` restores the newest
snapshot, so workspaces, people, invitations and files survive. Keeping data
costs little: snapshot storage for a small test database, two passwords
($0.40 each a month) and the files. A full environment costs roughly
$0.5–1 per hour (NAT, Aurora, load balancer, Fargate); measure it with `cost`.
Budget alerts warn but do not stop spending; `down` and `verify-clean` do.

## Access

| Identity | Used for | Permissions |
| --- | --- | --- |
| `cuckoo-cli` user | Signing in only | Assume the two roles below (after `lock-cli-user`) |
| `ayati-access-observer` | Reading state, logs and costs at any time | `ReadOnlyAccess`, cost/pricing/log queries; never secret values |
| `ayati-access-deployer` | Creating and deleting environments | ECS, ECR, EC2/VPC, RDS, ALB, logs, Route 53, ACM, CloudFront/WAF, budgets in `ap-south-1`/`us-east-1`; secrets and buckets named `ayati*`; roles named `ayati-*` only inside `ayati-runtime-boundary`. **Needs an MFA code**; sessions last 8 hours |

The deployer cannot read provider keys (WorkOS, model), remove the boundary,
change policies or touch the access roles. Changing these access policies
later needs the owner (root console), by design.

`access/apply.sh` creates the boundary and both roles (plan by default,
`--apply` to make changes). `apply.sh mfa-create` and `mfa-enable` give the
CLI user an MFA device; `apply.sh lock-cli-user --apply` then removes its
full-access policies.

## First time

```sh
cp deploy/aws/cloud.env.example deploy/aws/.env.cloud   # fill in; ignored by Git
deploy/aws/env.sh unlock 123456                          # MFA code
deploy/aws/env.sh foundation                             # prints four NS records
# Add the NS records for dev.cuckoo.onl at the registrar, wait until they resolve:
deploy/aws/env.sh foundation --with-certificate
deploy/aws/env.sh secret workos-api-key /private/path/workos-key
deploy/aws/env.sh secret model-api-key /private/path/model-key
deploy/aws/env.sh worker-tls
```

Add `https://app.dev.cuckoo.onl/auth/callback` as a WorkOS redirect URI.

## Each window

```sh
deploy/aws/env.sh unlock 123456   # if the 8-hour session expired
deploy/aws/env.sh image           # build and push the current commit
deploy/aws/env.sh up              # about 15–20 minutes, mostly Aurora
deploy/aws/env.sh check           # health and service counts
# ... test at https://app.dev.cuckoo.onl ...
deploy/aws/env.sh image && deploy/aws/env.sh deploy   # optional: roll a new commit
deploy/aws/env.sh down            # saves logs, snapshots, deletes, then verify-clean
deploy/aws/env.sh cost 7          # next day: what the window cost
```

`up` creates the kept stack once, restores the database from the newest snapshot
(or starts empty), creates the four temporary stacks, runs the setup task with
zero services, then starts the configured number of controllers and executors.
Setup migrates a restored database forward to the new image's schema; an older
image cannot open a newer database. `deploy` stops the
services, migrates and starts them again, because a controller accepts only the
exact schema it was built for. Logs are saved under
`~/.local/state/ayati/aws/<environment>/windows/`.

### Starting over

| Command | Effect |
| --- | --- |
| `env.sh up --fresh` | This window starts with an empty database and empty files; older snapshots stay until pruned |
| `env.sh down --discard` | Delete without keeping this window's database |
| `env.sh kept` | List the snapshots and files bucket being kept |
| `env.sh forget --yes` | Delete every kept snapshot, the files and the passwords (only when the environment is down) |

`up` never changes the snapshot of a database that already exists: rerunning it
after a failure keeps the same data. If a snapshot fails, `down` stops before
deleting anything and says so.

## Known limits (first windows)

- One NAT gateway and one Aurora writer: no zone failover until Step 8.
- No CloudFront, WAF or RDS Proxy yet; the controller serves the web client.
- PostgreSQL TLS is required but the server certificate is not verified
  (`sslmode=require`); add the RDS CA bundle to the image to verify it.
- First window must confirm: ECS task-role session names equal task IDs
  (executor identity), and CloudWatch extracts the EMF pool metrics.
- No automatic teardown: an environment runs until `down`.

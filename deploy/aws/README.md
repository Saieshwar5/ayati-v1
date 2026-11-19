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
          optional: ayati/e2b-api-key, ayati/composio-api-key,
          ayati/exa-api-key, ayati/secrets-key

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

## The fast cycle: park, wake, ship

Between working sessions, **park** instead of `down`, then **wake** the next
day. Ship each commit with one command:

```sh
deploy/aws/env.sh wake            # from parked: NAT on, Aurora resumed, services started, smoke
deploy/aws/env.sh ship            # image, deploy, smoke (refuses uncommitted changes)
deploy/aws/env.sh smoke           # readiness, counts, running image, new ERROR/WARN lines
deploy/aws/env.sh park            # logs saved; services, load balancer and NAT removed
deploy/aws/env.sh db-capacity     # Aurora capacity; 0 means paused
deploy/aws/env.sh reset --yes     # test data only: empty database and files, same environment
```

| State | What exists | Cost |
| --- | --- | --- |
| Down | Permanent layer and kept data | about $2/month |
| Parked | Plus the VPC without NAT, roles, Aurora at 0 ACU (paused after 5 idle minutes) | storage only, a few dollars a month |
| Awake | Plus NAT, load balancer, controllers and executors | about $0.5–1/hour |

- **Wake runs `up` on the parked environment:**
  - the database is reused, not restored;
  - raising Aurora's minimum capacity resumes it before setup connects;
  - setup retries once, because migrations connect with a 10-second timeout.
- **`ship` deploys by schema:**
  - **Rolling update:** used when the running commit has the same newest
    migration and the same `services.yaml` as the new one.
  - **Full deploy (stop, migrate, start):** used otherwise.
  - Force either with `--rolling` or `--full`; `--dirty` allows uncommitted
    changes.
- **Smoke fails** on a failed readiness check, on wrong counts, when an old image
  is still running, or on an ERROR or panic logged since the deploy started. It
  saves `smoke.txt` with the window's logs.
- **`verify-clean` while parked** lists the parked stacks as intentional and
  still reports any NAT, load balancer or ECS cluster left behind.
- **`down` works from parked too:** it snapshots, deletes everything and clears
  the parked state.
- **Parked means asleep:** schedules, agent wake-ups and events do not run.
- **`reset --yes` starts the data over:**
  - it stops the services, drops and recreates the database, empties the files
    bucket, migrates and starts again (a few minutes);
  - it refuses environment names containing `prod`, and refuses while parked;
  - sign-ins at WorkOS stay, so people simply sign in to an empty Cuckoo.

## Voice transcription (optional)

Controllers can transcribe voice notes ([voice transcription](../../runtime/voice-transcription.md)).
Set both in `.env.cloud`, then run `up` (or `wake` when parked). The first time, it must
be `up`: it updates `roles.yaml` before the services, and `deploy` and `ship` don't.

```sh
AYATI_TRANSCRIPTION_MODEL=fireworks-audio:whisper-v3-turbo
AYATI_TRANSCRIPTION_BASE_URL=https://audio-prod.us-virginia-1.direct.fireworks.ai/v1
```

- The controller's `prepare` container writes the `ayati/model-api-key` secret to a key
  file and a providers file naming that provider; the controller gets
  `AYATI_MODEL_PROVIDERS_FILE` (an empty list when this is off) and
  `AYATI_TRANSCRIPTION_MODEL`. The key never enters its environment.
- The provider must accept that same key. Fireworks' audio host is expected to take
  the Fireworks key (`AYATI_MODEL_PROVIDER=fireworks-ai`); the first window confirms it.
  Executors are unchanged.
- The controller's execution role may read that secret (`roles.yaml`); the task gets it
  only when transcription is on. Recordings are sent to the provider.

## Computers, connected apps, web search and app events (optional)

Four more keys switch these on. Each is stored once, from a private file that holds
the key alone on one line. A missing key leaves that feature off; nothing else changes.

```sh
deploy/aws/env.sh secret e2b-api-key ~/.config/cuckoo/keys/e2b            # agent computers
deploy/aws/env.sh secret composio-api-key ~/.config/cuckoo/keys/composio  # Gmail, Calendar, Sheets, Drive
deploy/aws/env.sh secret exa-api-key ~/.config/cuckoo/keys/exa            # web search and page reading
deploy/aws/env.sh secret secrets-key                                      # the sealing key, made once
deploy/aws/env.sh secrets                                                 # which are stored (never the values)
```

Then run `up` (or `wake` when parked), not `deploy` or `ship`: the first time,
`roles.yaml` must let the controller read the new secrets.

- **Controllers only.** The controller's `prepare` container writes each key to
  `/run/ayati/*` and writes the providers files from values in `services.yaml`
  (`COMPUTERS`, `CONNECTORS`, `WEB_SEARCH`); the controller gets
  `AYATI_COMPUTER_PROVIDERS_FILE`, `AYATI_CONNECTOR_PROVIDERS_FILE`,
  `AYATI_WEB_SEARCH_PROVIDERS_FILE` and `AYATI_SECRETS_KEY_FILE`. Executors never
  hold these keys. A Rust check (`identity::cloud_template_tests`) parses those values
  with the controller's own parsers, because a broken file stops the controller at start.
- **Computers** need three E2B templates, built once with the same key
  ([agent computers](../../runtime/agent-computers.md), step 3):
  `cuckoo-computer-small`, `-standard` and `-large`. Sizes and prices in `COMPUTERS`
  are test values: check E2B's prices, the plan's disk size and its longest sandbox
  time (`longest_work_minutes` is 55, under the one-hour limit of E2B's free plan).
- **Connected apps** use the Composio project key (it starts with `ak_`); each
  workspace is the Composio user `cuckoo-<workspace id>`.
- **The sealing key** (`ayati/secrets-key`, 64 hex characters) locks secrets kept in
  PostgreSQL. `secret secrets-key` refuses to replace it: a new key could not open
  what is stored. Deleting it means setting up app events again.
- **App events** (a new email starts an agent) need Composio, the sealing key and one
  action by a Cuckoo operator after the controller is up: open
  `https://<host>/control.html`, Providers, **App events**, **Set up app events**. That
  points Composio's one project webhook at `https://<host>/hooks/composio` and keeps
  its signing secret sealed. Do it again if the hostname changes.

## Known limits (first windows)

- One NAT gateway and one Aurora writer: no zone failover until Step 8.
- No CloudFront, WAF or RDS Proxy yet; the controller serves the web client.
- PostgreSQL TLS is required but the server certificate is not verified
  (`sslmode=require`); add the RDS CA bundle to the image to verify it.
- First window must confirm: ECS task-role session names equal task IDs
  (executor identity), and CloudWatch extracts the EMF pool metrics.
- No automatic teardown: an environment runs until `park` or `down`.
- RDS Proxy (D222) would keep Aurora awake while parked; park must remove it when
  it arrives.

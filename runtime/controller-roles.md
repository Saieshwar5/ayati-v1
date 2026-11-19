# Controller roles: web and work

The controller (`ayati identity`) does two different jobs. Serving people is many
short requests and long-lived live connections. Coordinating agents is job
hand-out, model traffic and background work that arrives in bursts. One setting
lets the two run as separate processes, so a busy or failed agent side does not
slow or stop Chat, and each side is sized and scaled on its own.

It is one binary, one image and one database. Built 2026-11-19; not yet run on AWS.

## The setting

`AYATI_CONTROLLER_ROLE` is `all` (the default), `web` or `work`.

| Part | `web` | `work` |
| --- | --- | --- |
| Browser port (`AYATI_PORT`): sign-in, pages, the browser API, webhooks | Yes | `/health/live` and `/health/ready` only; everything else is 404 |
| Live updates and presence | Yes | No |
| [Worker control](worker-control.md): executors, job hand-out, the [model gateway](model-gateway.md), tool calls, agent output | No | Yes |
| Scheduler (schedules, events, Library arrivals, wake-ups, panel updates) | No | Yes |
| Other background jobs: communication gateway, unconfirmed-write checks, cleanup and archive, connector drift, Library indexing and keeping, working summaries, lessons, waits for people, computer sleep, transcription | No | Yes |
| Pool metrics | Yes | Yes |

`all` runs everything in one process, exactly as before this setting existed. A
laptop keeps using it.

## How the two sides talk

They never call each other. One writes to PostgreSQL and PostgreSQL notifies the other:

- Web saves a message and its job in one transaction. The database's work
  notification ([schema 97](../src/identity/postgres/work_signals.rs)) wakes the
  work controller's dispatcher, which offers the job to an executor.
- Work saves the reply as it arrives. The message-change notification wakes
  web's live streams, which push it to browsers.

Notifications are hints. Each loop still reads its work from the database and
keeps a fallback timer (10 seconds for hand-out, 30 seconds for live streams),
so a lost notification delays work and never loses it. Every role holds one
listening connection for this.

## Settings by role

Each role refuses the other's secrets at startup, as executors already refuse
the controller's.

| Setting | `web` | `work` |
| --- | --- | --- |
| `RELAY_DATABASE_URL` | Required | Required |
| `WORKOS_CLIENT_ID`, `WORKOS_API_KEY` | Required | Refused |
| `AYATI_WORKER_CONTROL_URL`, `…_KEY_FILE`, `…_LISTEN`, `…_TLS_CERT`, `…_TLS_KEY` | Refused | Required as in [worker control](worker-control.md) |
| `AYATI_DATABASE_CONNECTIONS` | Its own pool size: 10 by default, 2–200 | Its own pool size |
| `AYATI_PUBLIC_ORIGIN` | Required when hosted | The same origin, for links in what agents write |
| `AYATI_BIND_ADDRESS` | The private address the proxy reaches | Leave unset: its browser port stays on loopback for the health check |

Give both roles the same pool limits (`AYATI_EXECUTOR_*`, `AYATI_ASSIGNMENT_*`),
model, catalog, connector and Library settings: web admits work into the queue
under the same limits the work side enforces. The total of every process's
`AYATI_DATABASE_CONNECTIONS` must fit the database's connection limit.

## Run it split

Locally, start two controllers with separate data directories and ports. One
`.env` holds both roles' secrets and each role refuses the other's, so turn the
file off (`AYATI_DOTENV=0`) and give each process only its own settings:

~~~sh
# Shared by both: RELAY_DATABASE_URL and the pool, model and Library settings.
AYATI_DOTENV=0 AYATI_CONTROLLER_ROLE=web AYATI_DATA_DIR=.ayati-local/web \
  WORKOS_CLIENT_ID=... WORKOS_API_KEY=... cargo run --locked -- identity
AYATI_DOTENV=0 AYATI_CONTROLLER_ROLE=work AYATI_PORT=8767 AYATI_DATA_DIR=.ayati-local/work \
  AYATI_WORKER_CONTROL_URL=http://127.0.0.1:8766 \
  AYATI_WORKER_CONTROL_KEY_FILE="$PWD/.ayati-local/worker-control.key" \
  cargo run --locked -- identity
~~~

With containers, add the overlay. `api` becomes the web controller and a new
`work` service serves executors on the private worker network:

~~~sh
docker compose -f deploy/compose.yaml -f deploy/compose.roles.yaml up -d \
  --scale work=2 --scale executor=4
~~~

## Known limits

- **Features that use a model inside a browser request stay on web.** The skill
  writer in the setup chat and Cuckoo Control's provider checks call a provider
  from the web process, so web still reads `AYATI_MODEL_PROVIDERS_FILE` and
  `AYATI_SECRETS_KEY_FILE` when those features are on.
- **Cuckoo Control's "resting" providers are per process.** A provider the model
  gateway rests after failures is remembered in the work process; the console on
  web does not show it.
- **The live picture of an agent's browser is per web process**, as it already
  was with several controllers.
- **One process still accepts at most 64 executors.** Add work controllers to go
  beyond that.
- **The database is shared.** Heavy agent writes can still slow reads for people;
  this split does not change that.

## Checks

`tests/controller_roles.mjs` runs a web controller, a work controller and an
executor on one database with a scripted provider ([tests](../tests/README.md)):

~~~sh
node --test tests/controller_roles.mjs
~~~

It checks that chat works with no work controller and requests wait; that a work
controller started later runs them; that work serves no pages; that each request
is picked up without waiting for the fallback timer; that web streams a reply
another process produced; that a schedule fires once; that losing the work
controller mid-reply loses nothing; and that schedules still run with web down.
The settings are covered by unit tests in `src/config/role.rs`.

First run, 2026-11-19 (debug build, PostgreSQL 16.15, scripted provider, one
shared host): all nine checks passed in 34 seconds. Work picked up four requests
110-136 ms after web accepted them, measured from the send to the job leaving
the queue. The existing single-process journeys (`multi_controller`,
`controller_drain`, `executor_pool`, `reply_stream`) gave the same results as
unchanged `main`. The load matrix (`tools/run-agent-load.py`) has not been run
split, so there is no capacity comparison between the two modes yet.

AWS is not changed yet: [deploy/aws](../deploy/aws/README.md) still runs one
`controller` service with the default role.

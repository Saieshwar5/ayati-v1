# Cuckoo Control (schema 87)

Cuckoo Control is the operator console for the whole platform: the Cuckoo team
monitors, configures and controls every workspace from one place. It is a web app
at `/control` and the same actions as `ayati admin` in a terminal. Both write one
change log. Built 2026-11-06.

It is not a workspace admin's Settings. Workspace admins manage one company;
Cuckoo operators (`AYATI_PLATFORM_ADMINS`) see every workspace. Operator APIs never
go through workspace membership, and workspace APIs never trust operator status.

**Operators see counts, names, settings and spend, never message contents or
Library files.** Problem reports are the exception: people write them for the
Cuckoo team.

## Who can open it

- The signed-in person's verified email must be in `AYATI_PLATFORM_ADMINS`. Others
  get 403 from every `/api/identity/admin/*` route and see a page that says why.
- The operator list is deployment configuration, never editable in the console, so
  someone who gets into the console still cannot add themselves.
- `ayati admin` acts as `AYATI_OPERATOR` (or `--as EMAIL`). That email must be in
  `AYATI_PLATFORM_ADMINS` and must have signed in to Cuckoo once.
- Every change needs a reason of at least four characters. The change log is
  append-only for the runtime database role (`REVOKE UPDATE, DELETE`).

## Screens

| Screen | What it shows | What it changes |
| --- | --- | --- |
| Overview | Active workspaces, calls per minute, failed calls, executor slots, spend today; model traffic per provider for the last hour; alerts; busiest workspaces | — |
| Spend | This month, forecast, today, per-day chart, by model (with unpriced calls), by workspace with its biggest agent | — |
| Problem reports | Reports from Settings → Help | — |
| Workspaces | Every workspace: owner, people, agents, 7-day calls, spend against cap, error rate, status. Detail: counts, models, Library size, operator history | Spending cap, suspend, lift suspension |
| Invite codes | The existing invite codes | Issue, revoke |
| Providers | Model providers with live health (calls, errors, p95 for the last hour), their models and prices; connected-app providers | Add a provider and its key, replace the key, remove, add or change models and prices, emergency switch |
| Routing | Routes, where traffic goes now (including switched-off providers), last-hour traffic per target; models agents use without a route | Add, change or remove a route |
| Configuration | Console settings with history; deployment settings read-only with their variable names | Console settings |
| Change log | Every change: who, what, why, web or CLI | — |

The top of every screen shows the environment band (`AYATI_CONTROL_ENVIRONMENT`:
red for production, amber for staging) and a live status strip with each
provider's health. ⌘K opens a command palette that finds workspaces and shows the
CLI command for each action.

## How the controls work

**Model providers.** A provider added in the console is stored in
`relay_model_providers` with its API key sealed by AES-256-GCM. The key comes from
`AYATI_SECRETS_KEY_FILE`: an owned 0600 file of 64 hex characters
(`openssl rand -hex 32`). The provider name is bound to the sealed key, so a key
cannot be moved to another provider's row. Keys are write-only: no API, page or CLI
output returns one; only the last four characters are shown. Providers in
`AYATI_MODEL_PROVIDERS_FILE` still work and show as read-only "deployment"
providers. Addresses must be HTTPS (loopback HTTP for local checks) and may not
contain credentials.

**Model catalog and prices.** `relay_platform_models` lists models per provider
with prices per million input and output tokens, and whether each is allowed. Each
call is priced when it is made (`relay_model_usage.cost_micros`), so changing a
price never rewrites history. A model with no entry still works; its calls show as
unpriced in Spend. A blocked model is skipped by the gateway and cannot be used in
a route.

**Routes.** A route is keyed by the model an agent asks for, so agents keep their
own model choice. It names a main provider and model, an optional second one, the
split (the main model's share) and a timeout. For each request the gateway:

1. Refuses the call if the workspace is suspended, has reached its monthly cap
   (its own, or `workspace.default_cap_usd`), or the platform has reached
   `platform.daily_ceiling_usd` today (UTC). These totals are at most 15 seconds old.
2. Orders the targets by the split. The split is per request (a stable hash of the
   request), so every call in one answer goes to the same model.
3. Skips targets whose provider is switched off or whose model is blocked.
4. Calls the first target. If the provider can't be reached, doesn't start
   answering within the timeout, returns 429 or a 5xx error, the call is recorded as
   an error and the next target gets the request. Other client errors are returned.
5. Records every attempt with its latency and outcome. Calls answered by a backup
   are marked `fell_back`.

Without a route, the assignment's own provider and model are used, as before. The
switches, catalog and limits still apply.

**Emergency switch.** `relay_model_provider_switches` turns a provider off for
every workspace. The next call skips it. Streaming replies finish. The console
shows which routes move to their backup and which would fail. Connected-app
providers (Composio) keep their existing switch; the console uses it and records
it in the change log.

**Suspension.** Suspending sets `relay_workspaces.suspended_ms`. The shared
membership check refuses everyone, so nobody can open the workspace. If agents
were not already paused, it pauses them with the existing pause and stops
unfinished requests. Lifting the suspension lets members back in and lifts only
the pause the suspension made; an admin's own earlier pause stays. Nothing is
deleted. The workspace's own audit log records both.

**Spending caps.** `relay_workspaces.spend_cap_micros` is a monthly cap in
micro-dollars; empty means the platform default. At the cap, new model calls are
refused with a message naming the limit. Caps reset on the 1st (UTC).

**Platform settings** (`relay_platform_settings`):

| Key | Effect |
| --- | --- |
| `signup.paused` | New workspaces are refused, even with an invite code; the code stays unused. Operators can still create one |
| `workspace.default_cap_usd` | Monthly cap for workspaces without their own; empty for none |
| `platform.daily_ceiling_usd` | Total model spend per UTC day; above it new calls are refused; empty for none |

Executor capacity, retention, the public address and the operator list remain
deployment settings, shown read-only with their variable names.

## Alerts

Alerts are worked out from stored facts on each overview read:

- a provider is switched off;
- a provider failed at least 5% of at least 20 calls in the last 15 minutes;
- a workspace has used at least 80% of its cap (marked "action" at 100%);
- the platform has used at least 80% of its daily ceiling;
- the oldest waiting request has waited over 120 seconds for an executor slot;
- problem reports arrived in the last day;
- unused invite codes expire within a week.

## Settings

| Setting | Where | Purpose |
| --- | --- | --- |
| `AYATI_PLATFORM_ADMINS` | Controller, CLI | Operators, by verified email |
| `AYATI_SECRETS_KEY_FILE` | Controller, CLI | Seals provider keys added in the console; without it the console shows providers but cannot store keys |
| `AYATI_CONTROL_ENVIRONMENT` | Controller | The environment band; defaults to `production` for HTTPS origins and `local` otherwise |
| `AYATI_OPERATOR` | CLI | The operator `ayati admin` acts as |
| `RELAY_DATABASE_URL` | CLI | The runtime database role, as for the controller |
| `AYATI_MODEL_PROVIDERS_FILE` | Controller, CLI | Deployment providers, still supported |

## CLI

```sh
ayati admin help
ayati admin overview
ayati admin workspaces [SEARCH]
ayati admin workspace suspend ID --reason "Chargeback"
ayati admin workspace cap ID 200 --reason "Paid plan"
echo "$KEY" | ayati admin provider add fireworks --url https://api.fireworks.ai/inference/v1 --reason "Main provider"
ayati admin model set fireworks accounts/fireworks/models/kimi-k2-instruct --input 0.60 --output 2.50 --reason "Price list"
ayati admin route set kimi-k2 --main fireworks:accounts/fireworks/models/kimi-k2-instruct --second openai:gpt-5-mini --split 90 --timeout 30 --reason "Test OpenAI"
ayati admin provider off fireworks --reason "Fireworks incident"
ayati admin setting set platform.daily_ceiling_usd 100 --reason "Budget"
ayati admin log --area routing
```

Keys are read from standard input, never from arguments, so they stay out of
shell history and process lists. `--json` prints the full answer for scripts.

## Checks

- Rust unit: key sealing (wrong secrets file, moved row, file permissions), the
  split (stable per request, close to the percentage), UTC month starts, CLI
  arguments, CLI time format.
- PostgreSQL (`src/identity/postgres/operator/`): suspension refuses members and
  lifts only its own pause; an admin's pause survives; calls are priced at call time
  and caps, the default cap and the daily ceiling refuse; pausing sign-up keeps
  the code; switches and failed calls show in health; the change log is
  append-only and needs reasons; routes record readable history and refuse
  blocked models and impossible splits.
- HTTP and gateway (`src/identity/operator/routes_tests.rs`): non-operators get
  403 and signed-out people 401 on every route; console keys are sealed and never
  returned; a route sends the request to the backup when the main provider fails
  (loopback providers), records both calls, prices the answer and honors switches;
  a suspended workspace is refused.
- Web (`web/tests/control.test.mjs`): non-operators see why; the environment band
  and status strip; turning a provider off needs a reason, shows the impact and
  sends once; a failed save keeps the dialog and reason; a route change waits in
  the tray until reviewed; suspension states what happens.
- Live, 2026-11-06: simulation server, Chromium, providers, prices and a route set
  with the CLI, then the web journey (switch off and on, route draft, workspace
  detail, spend, configuration, change log, ⌘K search) with no console errors, and
  no horizontal scroll at 390 px.

## Not yet

- Per-provider rate limits and monthly ceilings (the platform ceiling and workspace
  caps exist).
- Billing and revenue: Spend shows Cuckoo's model cost, not what customers pay.
- Operator roles (viewer, support, owner) and step-up authentication; every
  operator can do everything.
- Library embedding and summary calls are priced and counted, but don't follow
  routes or the emergency switch yet.
- Non-OpenAI protocols (Anthropic, Gemini) still need gateway adapters.
- Searching people across workspaces, and a reports inbox with "handled" states.

# Cuckoo Control (schema 91)

Cuckoo Control is the operator console for the whole platform: the Cuckoo team
monitors, configures and controls every workspace from one place. It is a web app
at `/control` and the same actions as `ayati admin` in a terminal. Both write one
change log. Built 2026-11-06 and rebuilt on the model catalog 2026-11-07.

It builds on the model work in schemas 87–90 and does not repeat it:

| From the model catalog work | What Cuckoo Control adds |
| --- | --- |
| [Catalog and routes](model-catalog.md): models people choose, providers that serve them, priority, weight, resting | A live view of the routes, and the **emergency switch** that takes a provider out of every route |
| [Charges and costs](model-usage.md) on every call | Spend across every workspace: charged, cost, margin, forecast |
| [Budgets](model-budgets.md) that workspace admins set | Budget alerts across every workspace, and a platform **daily ceiling** on provider cost |
| [Operator model view](model-operations.md): traffic, failures, speed, margin | Provider health on every screen and alerts |
| Providers file with key files | **API keys kept in the console**, sealed; extra providers outside the file |
| — | Workspaces (list, detail, **suspension**), **sign-up pause**, invite codes, problem reports, the **change log**, the CLI |

The catalog and the providers file stay reviewed files: the console shows them
live but does not edit them.

It is not a workspace admin's Settings. Workspace admins manage one company;
Cuckoo operators (`AYATI_PLATFORM_ADMINS`) see every workspace. **Operators see
counts, names, settings and money, never message contents or Library files.**
Problem reports are the exception: people write them for the Cuckoo team.

## Who can open it

- The signed-in person's verified email must be in `AYATI_PLATFORM_ADMINS`. Others
  get 403 from every `/api/identity/admin/*` route and see a page that says why.
- The operator list is deployment configuration, never editable in the console.
- `ayati admin` acts as `AYATI_OPERATOR` (or `--as EMAIL`), who must be in
  `AYATI_PLATFORM_ADMINS` and must have signed in to Cuckoo once.
- Every change needs a reason of at least four characters. The change log is
  append-only for the runtime database role (`REVOKE UPDATE, DELETE`).

## Screens

| Screen | Shows | Changes |
| --- | --- | --- |
| Overview | Active workspaces, calls per minute, failed calls and fallbacks, executor slots, charged and cost today; last-hour traffic per provider; alerts; busiest workspaces | — |
| Spend | Charged, cost and margin this month with forecasts; per day; by model; by workspace against its budget, with its biggest agent | — |
| Problem reports | Reports from Settings → Help | — |
| Workspaces | Owner, people, agents, 7-day calls, charged against budget, failure rate, status. Detail: counts, models, Library size, operator history | Suspend, lift suspension |
| Invite codes | The invite codes | Issue, revoke |
| Providers | Every provider: source, where its key is, last-hour health, the catalog models it serves; connected-app providers | Keep a provider's key here or replace it, add a provider outside the file, remove, emergency switch |
| Routing | The catalog's models and routes: priority, weight, cost, margin (losses in red), switched-off and resting providers, failures | — (the catalog is a reviewed file) |
| Configuration | Console settings with history; deployment settings read-only with their variable names | Console settings |
| Change log | Every change: who, what, why, web or CLI | — |

Every screen shows the environment band (`AYATI_CONTROL_ENVIRONMENT`: red for
production, amber for staging) and a live provider status strip. ⌘K finds
workspaces and shows the CLI command for each action.

## How the controls work

**API keys.** `relay_model_providers` keeps keys sealed with AES-256-GCM. The key
comes from `AYATI_SECRETS_KEY_FILE`: an owned 0600 file of 64 hex characters
(`openssl rand -hex 32`). The provider name is bound to the sealed key, so a key
cannot be moved to another provider's row. Keys are write-only: no API, page or CLI
output returns one; only the last four characters are shown.

- A row without an address is the key of a provider in the providers file. The
  gateway uses it instead of that provider's `key_file`, which is now optional.
  The address, and the catalog routes that name the provider, stay in the files.
- A row with an address is a provider added in the console, for agents that name a
  provider themselves. Catalog routes still name only providers in the file.
- Removing a file provider's key falls back to its key file. Cuckoo refuses when
  catalog routes need the provider and there is no key file.

**Emergency switch.** `relay_model_provider_switches` takes a provider out of every
route for every workspace: the gateway drops its routes before calling, so the
catalog's other providers serve those models. A model with no provider left is
refused with the operator's note. Streaming replies finish. The switch dialog lists
each catalog model and what serves it next. Connected-app providers (Composio)
keep their existing switch; the console uses it and records it.

**Suspension.** Suspending sets `relay_workspaces.suspended_ms`. The shared
membership check refuses everyone, so nobody can open the workspace. If agents
were not already paused, it pauses them with the existing pause and stops
unfinished requests, and the gateway refuses its model calls. Lifting it lets
members back in and lifts only the pause the suspension made. Nothing is deleted.
The workspace's own audit log records both.

**Platform settings** (`relay_platform_settings`):

| Key | Effect |
| --- | --- |
| `signup.paused` | New workspaces are refused, even with an invite code; the code stays unused. Operators can still create one |
| `platform.daily_ceiling_usd` | Provider cost per UTC day across every workspace; at it, the gateway refuses new calls. Empty for none |

Workspace and agent budgets remain the workspace admins' (Settings → Usage). Before
a call the gateway checks, in order: budgets, then suspension and the daily ceiling
(totals at most 15 seconds old), then the switch. Executor capacity, retention, the
public address, the catalog files and the operator list remain deployment settings,
shown read-only with their variable names.

## Alerts

Worked out on each overview read: a provider switched off; a provider resting on
this controller; a provider failing at least 5% of at least 20 calls in 15 minutes;
a workspace past its budget's warning share (marked "action" at 100%); provider
cost at 80% of the daily ceiling; work waiting over 120 seconds for an executor;
problem reports in the last day; unused invite codes expiring within a week.

## Settings

| Setting | Where | Purpose |
| --- | --- | --- |
| `AYATI_PLATFORM_ADMINS` | Controller, CLI | Operators, by verified email |
| `AYATI_SECRETS_KEY_FILE` | Controller, CLI | Seals API keys kept in the console; without it the console shows providers but stores no keys |
| `AYATI_CONTROL_ENVIRONMENT` | Controller | The environment band; defaults to `production` for HTTPS origins, `local` otherwise |
| `AYATI_OPERATOR` | CLI | The operator `ayati admin` acts as |
| `RELAY_DATABASE_URL` | CLI | The runtime database role, as for the controller |
| `AYATI_MODEL_PROVIDERS_FILE`, `AYATI_MODEL_CATALOG_FILE` | Controller, CLI | As in [the model catalog](model-catalog.md) |

## CLI

```sh
ayati admin help
ayati admin overview
ayati admin workspaces [SEARCH]
ayati admin workspace suspend ID --reason "Chargeback"
echo "$KEY" | ayati admin provider key fireworks --reason "Key moved to the console"
ayati admin provider off fireworks --reason "Fireworks incident"
ayati admin models --hours 24
ayati admin spend
ayati admin setting set platform.daily_ceiling_usd 100 --reason "Budget"
ayati admin log --area providers
```

Keys are read from standard input, never from arguments, so they stay out of
shell history and process lists. `--json` prints the full answer for scripts.

## Checks

- Rust unit: key sealing (wrong secrets file, moved row, file permissions), UTC
  month starts, CLI arguments, CLI time format.
- PostgreSQL (`src/identity/postgres/operator/tests.rs`): suspension refuses members
  and lifts only its own pause; an admin's pause survives; spend adds up charge, cost
  and margin, and the daily ceiling refuses; budgets past their warning share alert;
  pausing sign-up keeps the code; switches, console keys and failures show in the
  provider list; the change log is append-only and needs reasons.
- HTTP and gateway (`src/identity/operator/routes_tests.rs`): non-operators get 403
  and signed-out people 401; keys are sealed and never returned; a file provider
  without a key file uses the console key and one with a key file keeps it; the
  switch drops a provider's routes with the operator's note; a suspended workspace
  is refused.
- Web (`web/tests/control.test.mjs`): non-operators see why; the environment band and
  status strip; the switch dialog shows what serves each catalog model next, needs a
  reason and sends once; a failed save keeps the reason; Routing shows the catalog
  with losses, switched-off and resting providers and changes nothing; suspension.
- The model catalog's own checks (`tests/model_gateway.mjs` and its PostgreSQL tests)
  cover routing, fallback, charges, costs and budgets.

## Not yet

- Editing the catalog from the console: it stays a reviewed file.
- Per-provider rate limits; billing and invoices.
- Operator roles and step-up authentication.
- People search across workspaces, and a reports inbox with "handled" states.

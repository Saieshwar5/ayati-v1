# Connector provider check

Live checks of the connected-app providers behind
[connector providers](../../runtime/connector-providers.md). They read the
provider's account and the tools Cuckoo's catalog
([catalog.json](../../src/identity/connectors/catalog.json)) maps to it, and
change nothing: Composio gets GET requests only; Pipedream gets one token
exchange, then GET requests. Python standard library only, no install needed.

Keys come from the environment and are never printed or saved, in whole or in
part. The network must allow `backend.composio.dev` and `api.pipedream.com`.
Behind a proxy that re-signs TLS, point `SSL_CERT_FILE` at its CA bundle.

```bash
COMPOSIO_API=… python3 -I composio_basics.py
PIPEDREAM_CLIENT_ID=… PIPEDREAM_CLIENT_SECRETS=… PIPEDREAM_PROJECT_ID=proj_… python3 -I pipedream_basics.py
```

Both exit 0 when everything matches and 1 on any problem.

**Status:** tested against local fakes only (`tests/`). They await a first live
run with the deployment's keys; record what it finds in the runtime document
and fix the catalog where a name is wrong.

## composio_basics.py

Reads `COMPOSIO_API` (a Composio project API key).

| Line | Means |
| --- | --- |
| `key accepted: yes` | Composio answered `GET /api/v3.1/auth_configs` with the key. `no (HTTP 401)` stops the check |
| `project key: yes` | Not a personal user key (`uak_…`), which Cuckoo refuses. Only yes or no is printed |
| `auth configs: N` | Auth configs in the project (sign-in setups per toolkit) |
| `webhook subscription: yes` | The project has a webhook subscription (its event URL). `no`: none yet; `unknown`: neither the v3.1 nor the v3 listing answered |
| `tool SLUG (action): ok, version V` | The newest version takes every field Cuckoo maps, as plain values (text, numbers, true or false) |
| `… offered without some fields, missing fields a, b` | Cuckoo would offer the action without those optional fields; the catalog's names are probably wrong |
| `… not plain values a` | Those mapped fields are now lists or objects, which Cuckoo leaves out |
| `… not offered: Composio requires x, which Cuckoo does not map` | Composio requires a field Cuckoo doesn't offer, so the action is left out at connect. Also `deprecated`, or a field Cuckoo requires is gone |
| `… not found (HTTP 404)` | No tool by that name: fix `catalog.json` |
| `SUMMARY {"ok", "changed", "not_found"}` | Counts of the tool lines |

The comparison follows `connectors::curated::project`, the reduction Cuckoo
applies when an account connects.

## pipedream_basics.py

Reads `PIPEDREAM_CLIENT_ID`, `PIPEDREAM_CLIENT_SECRETS`, optionally
`PIPEDREAM_PROJECT_ID`, and `PIPEDREAM_ENVIRONMENT` (`development` unless set to
`production`).

| Line | Means |
| --- | --- |
| `credentials set: yes` | Both client values are present |
| `token exchange: accepted, expires in N s` | `POST /v1/oauth/token` with the client credentials returned an access token (never printed). `refused (HTTP 401)` stops the check |
| `environment: development` | The Connect environment used for reads (`x-pd-environment`) |
| `project: set` | With a project ID, the catalog's Pipedream entries are read; without one the check ends here |
| `catalog entries for pipedream: N apps, M actions` | What the catalog maps to Pipedream (none until the Pipedream provider adds mappings) |
| `app SLUG (app): found` | `GET /v1/connect/{project}/apps/{slug}` answered |
| `component KEY (action): ok, version V` | The component takes every mapped prop, and every prop it requires is mapped (app sign-in props belong to the account) |
| `… missing fields a` / `… requires x, which Cuckoo does not map` | The mapping needs fixing before the action can be offered |

The Connect read paths are written from Pipedream's documentation and are
unconfirmed until the first live run.

## Tests

```bash
python3 -I -m unittest discover -s tests
```

Each script runs against a local HTTP stand-in: accepted, refused and personal
keys, field comparisons, and that no key, secret or token appears in the output
or in a request path.

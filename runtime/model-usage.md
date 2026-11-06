# Model usage and charges (MG2, schema 88)

Every model call through the [gateway](model-gateway.md) is saved with who it was for and
what it was charged. People see their use by agent, model, person and day. Built 2026-11-06.

## What each call records

`relay_model_usage` gains three columns (schema 88):

| Column | Meaning |
| --- | --- |
| `person` | Who the work was for: the request's requester, or the actor of a task. Library indexing has none. |
| `charge_micro` | What the call is charged, in millionths of a dollar: tokens read × the catalog `price.input` plus tokens written × `price.output`. With prices in dollars per million tokens, millionths of a dollar are simply tokens × price. |
| `cost_micro` | What the provider charges us, computed the same way from the `cost` of the [route](model-catalog.md) that answered. It is for the operator view (MG4) and never sent to people. |

Prices are saved at the moment of the call, so a later price change never rewrites
history. Models outside the catalog, and Library indexing, are charged 0. Cached input
prices aren't applied yet: the gateway reads only total input and output tokens.

## Who sees what

`GET /api/identity/workspaces/{w}/usage?days=30[&agent=ID]` (1 to 90 days) returns
`{days, since_ms, scope, totals, by_agent, by_model, by_person, by_day}`. Each row has
`calls`, `input_tokens`, `output_tokens` and `charge_usd`.

| Viewer | Without `agent` | With `agent` |
| --- | --- | --- |
| Admin | The whole workspace (`scope: workspace`) | That agent |
| The agent's owner | Their own use (`scope: mine`) | That agent |
| Other members | Their own use (`scope: mine`) | Refused |

Days follow the workspace's time zone (UTC until an admin sets one). Agents and people
are named, and catalog models carry their catalog names. Library calls show as
"Library", and calls with no person show as "Workspace".

## Where it shows

- **Settings → Usage**: admins see the workspace and members see their own use, with a
  7, 30 or 90 day period, totals, and tables by agent, model, person and day.
- **Agent → Environment → Usage**: the same for one agent, shown to its owner and admins.
- **Activity → request details**: "Model use: … · 2 calls · 1,240 tokens in, 312 out · $0.0042".
- Charges smaller than $0.0001 read "under $0.0001", never $0.00.

## Checks

- Unit: tokens × price in millionths of a dollar.
- PostgreSQL (`postgres::model_usage_report_tests`): admins see the totals for the whole
  workspace, most charged agent first. Members see only their own use. An agent's owner
  sees that agent, and members can't. Strangers and periods over 90 days are refused.
  Reports never include costs.
- Real Goose (`tests/model_gateway.mjs`): the usage row's charge equals tokens × catalog
  price, its cost equals tokens × route cost, and the person is the requester. `/usage`
  names the catalog model and carries no cost. A browser check covers Settings → Usage. The
  fixture provider now sends OpenAI's final usage chunk when asked for it.
- Web (`web/tests/model-usage.test.mjs`): totals and the four tables; a slow answer for an
  old period never replaces the current one; members get no per-person table; empty periods
  and tiny charges.

## Next

- MG3 (built): [monthly budgets](model-budgets.md) per workspace and agent.
- MG4 (built): the [operator view](model-operations.md), with costs, margin and provider health.
- Billing: invoices from these charges, once pricing (markup or credits) is decided.

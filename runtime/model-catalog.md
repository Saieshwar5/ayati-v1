# Model catalog and routing (MG1, schema 87)

People choose **models**; Cuckoo chooses **who serves them**. Each model in the catalog
has Cuckoo's own id and name, and private **routes** to one or more providers
(Fireworks, OpenAI, OpenRouter, or any other OpenAI-compatible API). People never see a
provider, and nobody's agent changes when we move a model to another provider. This builds on the [model gateway](model-gateway.md):
the controller makes every model call, so it alone picks the route. Built 2026-11-06.
Off unless `AYATI_MODEL_CATALOG_FILE` is set.

## Turning it on

| Where | Setting |
| --- | --- |
| Controller | `AYATI_MODEL_PROVIDERS_FILE` as for the gateway. Each entry may add `"protocol": "openai"` (the default and, for now, the only one accepted). |
| Controller | `AYATI_MODEL_CATALOG_FILE`: the catalog below. It needs the providers file. Both files are read when used, so edits apply without a restart. |
| Executors | `AYATI_MODEL_GATEWAY=1` and `AYATI_MODEL=*`: the executor reports the profile `gateway/*` and takes work for **any** model. The model is chosen per call by the controller. `AYATI_MODEL=*` without the gateway is refused. Executors with a named model keep working as before. |

## The catalog file

```json
{"models": [{
  "id": "llama-3.3-70b", "name": "Llama 3.3 70B", "family": "Llama",
  "description": "Quick everyday answers.", "tags": ["fast"], "context": 128000,
  "supports": {"tools": true, "vision": false},
  "price": {"input": 0.9, "output": 0.9},
  "status": "available",
  "routes": [
    {"provider": "fireworks", "model": "accounts/fireworks/models/llama-v3p3-70b-instruct", "priority": 1, "weight": 80, "cost": {"input": 0.9, "output": 0.9}},
    {"provider": "openrouter", "model": "meta-llama/llama-3.3-70b-instruct", "priority": 1, "weight": 20},
    {"provider": "together", "model": "meta-llama/Llama-3.3-70B-Instruct-Turbo", "priority": 2}
  ]
}]}
```

- `id`: lowercase letters, numbers, `.`, `_` and `-`. This is what agents save.
- `price` (what people are charged) and `cost` on a route (what the provider charges us)
  are in US dollars per million tokens, from 0 to 1,000. `cached_input` is optional. MG2 uses them.
- `status`: `available`, `preview` or `retiring`. A retiring model can't be chosen again
  and may name a `replacement`. Agents already on it are told to move.
- 1 to 8 routes per model. Each must name a provider from the providers file.
  `priority` runs from 1 to 9 and defaults to 1; `weight` runs from 1 to 1,000 and defaults to 100.
- Unknown fields are errors, so a typo is never silently ignored.

## How a call is routed

1. The gateway takes the assignment's model as before. If it is in the catalog, its
   routes are ordered (`src/identity/models/router.rs`): first by priority. Within a priority,
   a weighted shuffle spreads traffic by weight.
2. A provider that just failed **rests** for 30 seconds: its routes go last, so they are
   tried only if every other route also fails. Health is kept in each controller's
   memory and only affects ordering.
3. The controller sends the call to the first route, under that provider's own model name.
   On 429, 408, a 5xx status or a connection error, it moves to the next route. This
   happens only before any of the answer has been forwarded, so a reply is never mixed
   from two providers. Other errors (400, 401, 404) are passed straight back.
4. Usage rows (`relay_model_usage`) now also save `provider_model` (the provider's name for
   the model) and `attempts` (how many routes were tried, from 1 to 9). `provider` is the route
   that answered.

A model that isn't in the catalog uses the assignment's provider as before.

## What people see

- `GET /api/identity/workspaces/{w}/models` (members):
  `{catalog_on, models:[{id, name, family, description, tags, context, supports, price, status, replacement}]}`.
  It never includes routes or costs. The agents directory carries the same value as `catalog`.
- Agent → Environment shows the catalog models allowed in the workspace, with their
  name, tags, description, abilities and price. Saving stores the model id with an empty provider.
- Settings → Workspace: admins limit the workspace to some catalog models, chosen
  from a list. Entries that still name a provider are marked and must be removed.
- New agents start on the workspace default. If that model can't be chosen, they start on
  the first catalog model that isn't retiring.

With a catalog, the server enforces all of this. Agent saves must name a catalog model that
isn't retiring and leave the provider empty ("Cuckoo picks who serves it"). Workspace model lists may
only hold catalog models. The workspace's allowed-model list still applies.

## Open source considered

[LiteLLM](https://github.com/BerriAI/litellm) (MIT core) offers a proxy with routing,
fallbacks and spend tracking. It wasn't made a required part of Cuckoo for three reasons.
Fireworks, OpenAI, OpenRouter and Together all accept OpenAI's request format. Routing,
usage and limits must stay in the controller, which alone knows the assignment, agent,
person and workspace. And a second service would add another place that holds keys.
It remains the planned way to reach APIs that don't speak OpenAI's format (Anthropic, Gemini, Bedrock):
you run it as one more OpenAI-compatible provider in the providers file (MG5), after a real
integration check that records the tested version and its limits.

## Checks

- Unit (`identity::models`): catalog validation and the public view; route order by
  priority and weight; resting providers go last; which statuses try another route.
- PostgreSQL (`postgres::model_catalog_tests`): with a catalog, unknown models, a
  named provider and retiring models are refused, and catalog models are accepted.
  Workspace lists take only catalog models. A `gateway/*` executor slot takes work that a
  named-model slot can't.
- Real Goose (`tests/model_gateway.mjs`): the first route's provider returns 503, so the call
  moves to the second route under its own model name. The executor runs with `AYATI_MODEL=*`.
  The usage row records the route that answered and 2 attempts, and `/models` shows no providers.
- Web (`web/tests/model-catalog.test.mjs`): the picker shows catalog models, prices and a
  notice for a retiring model. It saves no provider, saves once on a double click and keeps
  the choice after a failure. Settings adds only catalog models.

## Next

- MG2 (built): price and cost per call, and usage by agent, model, person and day ([model usage](model-usage.md)).
- MG3 (built): [monthly budgets](model-budgets.md) per workspace and agent.
- MG4 (built): the [operator view](model-operations.md) of provider health, errors, speed and margin.
- MG5: an optional LiteLLM route for APIs that don't speak OpenAI's format.

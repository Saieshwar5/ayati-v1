# Model operations for operators (MG4, schema 90)

Cuckoo's platform administrators (`AYATI_PLATFORM_ADMINS`) see how the providers behind
the [model catalog](model-catalog.md) are doing across every workspace: traffic, fallbacks,
failures, speed, and what people were charged against what providers charged us.
Nobody else can see it. Built 2026-11-06.

## What is recorded

- `relay_model_usage.latency_ms`: time from sending the call to the end of the
  answer, for the route that served it.
- `relay_model_failures`: every call a provider failed, with its workspace, provider,
  catalog model, the HTTP status (0 when unreachable) and the time.
  - A 429, 408 or 5xx also rests the provider ([router](model-catalog.md#how-a-call-is-routed)).
  - Any other refusal (for example 400) is counted without resting the provider.
  - Recording never blocks the call.

## Where it shows

Account → **Cuckoo admin** (`/?view=account&admin=1`), under the invite codes. It covers
the last 24 hours, 7 days or 30 days, from `GET /api/identity/admin/models?hours=` (1 hour to 31
days). Anyone who isn't a platform administrator gets 403.

| Section | Shows |
| --- | --- |
| Resting | Providers resting after a failure on the controller that answered |
| Providers | Calls, calls served after a fallback, tokens, average and 95th-percentile time, charged, cost and margin. Losses are in red. |
| Failures | Per provider and answer (503, 429, Unreachable…): how many and when last |
| Models | The same figures per catalog model |
| Catalog routes | Each model's routes with provider name, priority, weight, price, cost and margin per million tokens. A route that loses money is in red, and a catalog that can't be read says why. |

Library indexing is the controller's own work, so it's left out of provider traffic.

## Checks

- PostgreSQL (`postgres::model_operations_tests`): traffic from two workspaces is added up
  per provider and model, without Library calls. It covers fallbacks, charged, cost,
  margin, average and 95th-percentile time, failures counted per status, and the time window.
- Real Goose (`tests/model_gateway.mjs`): the down provider's 503 is recorded and calls
  carry a time. The operator API shows the serving provider with a positive margin, the
  failure and the route margins. Other people get 403, and the page renders for the
  platform administrator.
- Web (`web/tests/model-operations.test.mjs`): the sections and losses; a slow answer for
  an old period never replaces the current one; a broken catalog is reported.

## Not yet

- Alerts when a provider's failures or speed cross a threshold.
- Editing the catalog from this page: the catalog stays a reviewed file.

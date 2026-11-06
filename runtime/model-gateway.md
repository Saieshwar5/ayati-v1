# Model gateway (schema 68)

With the gateway on, agents reach AI models **only through the controller**. Executors
hold no provider key and no provider address; the controller adds the key from its own
file, uses the assignment's model, streams the answer back and records token use. From
the core-agent plan (CA3, draft decision D240). Built 2026-11-05. Off by default.

## Turning it on

| Where | Setting |
| --- | --- |
| Controller | `AYATI_MODEL_PROVIDERS_FILE`: a JSON list of `{provider, base_url, key_file}`. `base_url` is the provider's OpenAI-compatible base (`https://api.openai.com/v1`, `https://api.fireworks.ai/inference/v1`); HTTPS, or loopback HTTP for local checks. Key files are read on every request, so keys rotate in place. |
| Controller (Library) | `AYATI_LIBRARY_EMBEDDING_MODEL` and `AYATI_LIBRARY_SUMMARY_MODEL` (`provider:model`): the controller's own calls for Library meaning search and document summaries, with keys from the same file ([Library](library.md)). |
| Executors | `AYATI_MODEL_GATEWAY=1`. Executors then **refuse** `OPENAI_API_KEY`, `FIREWORKS_API_KEY`, `OPENAI_HOST` and `OPENAI_BASE_PATH`. |

The controller still refuses `OPENAI_API_KEY` and `FIREWORKS_API_KEY` in its
environment; keys live only in the key files.

## How a request travels

```
Goose ──OpenAI protocol──▶ slot's loopback proxy ──worker control──▶ controller ──▶ provider
      (per-slot random key)   (armed for one assignment)   (assignment check,     (real key)
                                                          model, key, usage)
```

1. Each executor slot starts a loopback proxy (`executor/model_proxy.rs`). Goose — both the
   warm chat process and tool runs — is launched with `OPENAI_HOST` pointing at it and a
   random per-slot key, and speaks OpenAI's protocol whatever the upstream provider.
2. The proxy refuses everything unless it's armed: a slot arms it for its current
   assignment and disarms it when the run ends. It accepts only `POST …/chat/completions`
   with its own key, the right host, and no Origin or cookie; `GET …/models` returns an
   empty list.
3. It forwards the exact request to `POST /v1/assignments/{id}/model` on worker control,
   with the assignment's capability (same check as tools and output: credential,
   assignment scope and session). Requests up to 8 MiB.
4. The controller uses the **assignment's model** (from its prepared context), whatever
   Goose asked for, and its provider (or the first worker provider). For streamed
   answers it asks for usage in the last chunk.
5. The answer streams back as it arrives. When it ends, the controller reads the token
   counts from the JSON answer or the last usage chunk and saves a row in
   `relay_model_usage` (workspace, assignment, agent, request, provider, model, tokens).
6. Provider errors pass through with their status; an unreachable provider is 502.

## Where it shows

Activity → a request's details shows "Model use: fixture · 2 calls · 1,240 tokens in, 312
out" (`model_usage` on `GET …/activity/requests/{reply}`).

## Checks

- Unit: usage from JSON and from the last streamed usage chunk; executors in gateway mode
  refuse model keys and provider addresses.
- Real Goose (`tests/model_gateway.mjs`): the provider only accepts the controller's key;
  an executor started with no key and no provider address answers a plain chat (warm
  slot) and a tool run; the assignment's model is used; usage rows are saved and shown
  on the request; the key never appears in any process log.
- Without the gateway, existing journeys (for example `tests/agent_tasks.mjs`) run as
  before.

## Not yet

- Choosing a model per agent beyond the assignment's provider/model profile, and
  fallbacks between providers.
- Non-OpenAI protocols (Anthropic, Gemini) as adapters.
- Prices and cost per task, budgets and the embedding and classification slots.

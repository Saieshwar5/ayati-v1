# Bounded provider calibration

This opt-in developer workflow runs synthetic conversations through the verified
deployment image, existing controller/executors and actual Chat browser. Scripted
mode is the default. Real-provider execution requires an explicit plan, private
key file and `--live`; an ambient model key cannot silently enable it. No normal
chat behavior, production billing system or AWS deployment is changed.

## Rehearse without paid calls

Use the prerequisites from [image verification](release-image-validation.md), plus
the existing `web/node_modules` Playwright installation and its Chromium browser.
Pass an immutable local image ID that has already passed image verification:

~~~sh
node tools/calibrate-provider.mjs --image sha256:...
~~~

`--check` prints the selected plan and reservation arithmetic without starting
Docker, a database or a provider request. `--help` prints usage. The default uses
only a loopback scripted upstream; its USD numbers are test arithmetic, not charges.
The worker-control network remains the existing same-host loopback transport.
Image roles retain their restricted user/root filesystem, private PID namespaces,
actual resource ceilings and narrowly owned cleanup.

## Review and run a live plan

Copy [the deliberately incomplete example](provider-calibration.example.json) to
a private file and replace every placeholder. Select `openai` or `fireworks`, an
explicit text Chat Completions model, current token prices in USD per million and
the total allowance. Record an official pricing URL and a checked date within
seven days. No model, commercial allowance or current rate is selected for you.

The first adapter supports only the standard HTTPS Chat Completions endpoints at
`api.openai.com/v1/chat/completions` and
`api.fireworks.ai/inference/v1/chat/completions`. Both use the existing Goose
`openai` compatible runtime profile. The report separately records the actual
upstream provider/model; this does not prove the native `fireworks-ai` adapter.
Responses-only models, custom endpoints and GPU-time-priced deployments need
separate adapters. Unsupported parameters/models fail visibly; no model fallback
or upstream retry is added by the proxy.

Keep the provider key in an owner-only regular file readable by your current
account (for example mode 0600). Its value is never a CLI argument or report field.
The private host proxy loads it; workers receive a generated temporary proxy key
through the existing secret-file mechanism. No provider key enters a service
container or the browser. Redirects are refused and TLS verification is required.

~~~sh
node tools/calibrate-provider.mjs --image sha256:... --live \
  --plan /private/calibration.json --key-file /private/provider.key --check

node tools/calibrate-provider.mjs --image sha256:... --live \
  --plan /private/calibration.json --key-file /private/provider.key
~~~

Plan limits constrain request count (7–24), concurrent upstream requests (2–4),
prompt bytes, requested output tokens (128–2048), per-call duration (10–120 seconds)
and a small declared USD allowance (at most 100 USD). The workflow needs allowance
for seven calls. Every additional inference attempt/retry still passes through the
same limits. The proxy has no retry loop; a retried Goose/controller attempt can
consume another reservation and makes the healthy single-inference check fail.
The maximum potentially forwarded count is printed during preflight.

## What is measured

Seven planned model requests exercise two individual first-turn chats (small and
larger scoped context), two concurrent agents, background work plus a short chat,
and a streamed reply cancelled with the actual Chat Stop button. Each uses a fresh
synthetic agent/direct conversation: Ayati deliberately reuses an existing direct
conversation for the same agent. No real customer data or business tools are used.

For five healthy foreground samples, the report correlates browser send/ack,
database acceptance/reservation/context/first saved output/final save, existing
controller/executor start traces, proxy forward start, upstream headers, first
text parsed from the upstream stream and first text visible in the Chat DOM near
a rendering frame. Proxy forward start is not the remote provider's internal
receipt/inference timestamp. Browser DOM observation is not a physical display
measurement. First reasoning/role/empty chunks do not count as visible text.

Raw cases, p50/p95/p99 summaries and actual cgroup resources are retained. Five
samples are exploratory: their p95/p99 are effectively the slowest sample and do
not establish a provider SLA, production capacity or statistically stable latency.
The report records whether the two model streams overlap and requires the mixed
short model request to begin while background inference remains active. A reply
that finishes before Stop, missing usage or an unobserved mixed overlap leaves
the scenario incomplete; the runner does not silently spend more to force a pass.

## Cost accounting and interruption

Before any upstream call, synchronously reserve the declared input-token allowance
plus maximum output tokens at the declared rates, using integer micro-USD arithmetic.
Reservations are retained before forwarding. They are **never refunded**, including
after cancellation, timeout, HTTP failure, missing usage or a retry. Concurrent
requests reserve before yielding. Exceeding reported input/output allowance fences
further forwarding. Only text messages and a single completion are forwarded;
model/output limits override harness fields and optional paid features are omitted.

`input_token_allowance` is an operator-verified conservative assumption, not an
input-token limit enforced by Chat Completions. Use the selected model's documented
maximum input/context allowance and verify its pricing/billing mode. A byte limit
is not an exact token count. Consequently this reservation ledger is **not a hard
provider billing cap**: incorrect rates/allowances or provider billing behavior can
exceed the estimate. Provider-side controls and the actual invoice remain separate.
Reported token counters produce a usage cost estimate, not a billing receipt.

The proxy requests streamed usage. An aborted stream can omit its final usage
chunk, so Stop aborts the local upstream request and reports unknown usage while
retaining the full reservation. It does not promise generation stopped remotely,
a refund or zero further provider billing. This follows the documented
[OpenAI streaming usage contract](https://developers.openai.com/api/reference/resources/chat/subresources/completions/streaming-events).
[OpenAI output limits](https://developers.openai.com/api/reference/resources/chat/subresources/completions/methods/create)
include reasoning tokens; verify compatibility for the selected model.
[Fireworks' contract](https://docs.fireworks.ai/api-reference/post-chatcompletions)
provides its compatible streaming/usage and output parameters. Model-specific live
acceptance remains required.

## Retained evidence and next gate

Each `.tmp/provider-calibration-*` directory is private (0700), with the plan/mode,
immutable image identity, bounded-run pointer, timing/cost reports, provider ledger,
run log and a synthetic Chat screenshot. The loopback proxy records marker IDs,
byte counts, timings, status and numeric usage; it does not retain prompt, output,
reasoning, key values or raw provider error bodies. The fresh fixture database
still contains synthetic conversations, and existing process logs stay private.
The bounded runner records source hashes, actual container limits and cleanup.
Images, evidence and stopped private data remain for review; unrelated Docker
resources are preserved. A SIGINT/SIGTERM at the wrapper requests bounded-run cleanup.

Passing scripted rehearsal proves the tooling and existing application path, not
live provider latency. After a bounded live report is accepted, use observed
durations/occupancy with the existing load profiles for final M0 sizing/IaC. M2/M3
must still prove private AWS worker identity before real context, then service
startup, scale-out, drain and recovery. No AWS resources are needed for this step.

## Verified local rehearsal — 2026-10-24

Final evidence: `.tmp/provider-calibration-r5Ked8/verification.json` and
`calibration-report.json`; bounded runner `.tmp/bounded-agent-l7sd9qh5`.
The same immutable image from the [release-image gate](release-image-validation.md)
was reused. All seven planned scripted calls pass, including five healthy
foreground samples, observed concurrent/mixed overlap and actual Stop terminating
the assigned Goose process and local upstream connection. The cancelled request
has unknown usage and retains its full reservation. No paid provider was used.

Exploratory browser-click-to-first-visible p50/max was 418/595 ms, including the
scripted upstream's intentional 100 ms delay and the normal Chat delivery path.
Server acceptance to dispatch p50/max was 12/13 ms. These are five local samples
with a browser and proxy inside the limited test-driver scope, not real-model
latency or a controlled comparison with earlier API-only load matrices.

Thirteen boundary checks and nine Python resource/ownership guards (normal and
optimized) pass. The eleven existing image/mixed/fault journeys pass in
`.tmp/bounded-agent-cxa_apct`, including the existing 96-chat mixed workload.
An explicit wrapper SIGTERM during a forwarded request produces the expected
interrupted result (130), then stops owned resources. Evidence:
`.tmp/provider-calibration-verification/interrupt-check.json` and `final-audit.json`.
The audit verifies 339 final source hashes and cleanup of all rehearsal resources;
the 16 existing stopped Docker containers remain. The first direct-chat reuse
fixture failure and intermediate runs are retained separately from final evidence.
That scripted checkpoint did not establish live acceptance. The following trial
records the first real-provider result.

## First live Fireworks profile — 2026-10-24

The owner authorized Fireworks, a cheapest-model trial and the key in the root
`.env`. The key authenticated against Fireworks' model catalog. Selected
`accounts/fireworks/models/nemotron-lightning-3p5-30b-a3b`, an available serverless
chat model with the lowest published Standard input rate among the current
candidates: $0.05/M input and $0.20/M output. This input-heavy workload favors that
rate. Sources: [current pricing](https://docs.fireworks.ai/serverless/pricing) and
[model card](https://app.fireworks.ai/models/fireworks/nemotron-lightning-3p5-30b-a3b).
Standard is the default; no Priority/Fast selection or GPU deployment was used.

The unmodified calibration code at `f8072afc` passed its first live run. All seven
requests returned HTTP 200 with the selected served model: six completed and one
was intentionally cancelled. Five healthy foreground samples cover individual
small/larger context, two concurrent agents and chat beside active background work.
Both overlaps were observed. Chat Stop persisted cancellation, terminated the
assigned Goose process and closed the local upstream stream in 64 ms.

| Local measurement | Median | Slowest of five |
| --- | ---: | ---: |
| Server acceptance to ready-worker dispatch | 14 ms | 20 ms |
| Server acceptance to proxy forward | 86 ms | 144 ms |
| Proxy forward to first provider answer text | 1,159 ms | 3,167 ms |
| Browser Send click to first visible text | 1,326 ms | 3,637 ms |
| First saved text to visible browser text | 22 ms | 29 ms |

The provider interval includes Internet transport, provider queueing, reasoning
and generation; it does not isolate inference compute. The model's documented
default includes reasoning, and no reasoning-disable option was added. Five samples
are exploratory, not stable p95/p99, AWS latency or a production SLA. The synthetic
company UI retains its simulation/scripted-agents banner, but the audited proxy
forwarded these requests to real Fireworks inference. Normal workspace settings,
ordinary databases and the root `.env` were not modified.

The plan allowed at most seven calls, two concurrent, 32,768 prompt JSON bytes and
1,024 output tokens, with a 65,536-token input reservation and $0.025 allowance.
The smaller input reservation is conservative for the byte-bounded synthetic text
plus template overhead; it is not the model's full 262k context or a provider token
limit. Usage stayed below it. Six completed requests reported 8,636 input and 2,843
output tokens. At declared uncached rates, per-request rounded usage estimates sum
to **$0.001004**; cache discounts and the actual invoice are not reconciled.
Cancelled usage is unknown and keeps **$0.003482** reserved. All nonrefundable
reservations together are **$0.024374**; that total is an accounting allowance,
not the actual charge. There were no denials, excess-usage fences or extra retries.

Retained local evidence:

- `.tmp/provider-calibration-rFrwT5/verification.json` and `calibration-report.json`.
- `.tmp/bounded-agent-licquvz8` for source/resource/container/cleanup evidence.
- `.tmp/fireworks-live-verification/final-audit.json` and private selected plan.
- `.tmp/provider-calibration-rFrwT5/chat-small.png` for the actual model reply in Chat.

The immutable image is unchanged from the release-image gate. The audit matches
339 source hashes, verifies the key used matches `.env` without displaying it,
and confirms all eight registered test containers, both scopes and the private
postmaster are stopped. All 16 existing stopped containers remain. The temporary
provider-key copy is removed after verification; the original `.env` is preserved.
No AWS resources were created. This accepts one bounded local Fireworks-compatible
profile, not the native `fireworks-ai` adapter, business tools or all providers.

Next: prepare M0's reviewable AWS resource/cost/IaC manifest, then prove M2/M3 private
service identity before live cloud context. Retain this first profile alongside
the larger scripted resource trials; broader live sampling, model/think-mode
latency comparison, cloud scaling and multi-hour acceptance remain separate gates.

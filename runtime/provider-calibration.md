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
Live model/rates/allowance selection and real-provider acceptance remain pending.

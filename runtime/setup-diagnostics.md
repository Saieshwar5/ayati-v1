# Local setup diagnostics

Open Settings → Local setup. `GET /api/setup` describes the default agent's
startup configuration: provider, model, execution mode, credential presence,
and pinned Goose version (host) or manifest/files and KVM access (VM).
It does not boot a guest, send inference, or read saved conversations. Checks
are observations, not a guarantee of a working VM or provider account. Invalid
configuration that prevents daemon startup must still be diagnosed in the terminal.
The guide currently supports Goose; the legacy prototype is explicitly unsupported.

Configure credentials in the local `.env` and restart Ayati. Fireworks uses
`AYATI_GOOSE_PROVIDER=fireworks-ai`, `FIREWORKS_API_KEY` and `AYATI_MODEL`.
No secret values are returned to the browser. Follow README.md for host/VM setup;
preserve old VM disks and agent data when repairing setup.

The checkbox and **Test provider connection** explicitly authorize one external
request. `POST /api/setup/test` requires `{"confirm":true}` and accepts an optional `model` candidate. Without
a candidate it uses the default agent's current saved model. It sends
only `Reply with OK.` to the default configured model, with no tools/history,
`stream:false`, and a 64-token output limit. Normal API charges may apply.
A successful response must contain nonempty text. Reasoning models can exhaust
this small limit; a failed check does not necessarily mean the model is unusable.

The endpoint uses the same provider URL rules as the VM gateway: fixed Fireworks
endpoint or configured OpenAI-compatible host/base path. Redirects are disabled,
HTTPS is required except for loopback fixtures, the timeout is 30 seconds, and
response parsing is bounded to 64 KiB. Provider response text and error bodies
are never returned or logged. Only one test can run at a time per workspace.
There are no automatic retries or durable test receipts: after a lost connection,
the provider may have received the request. A new explicit test may cost again.
Shutdown cancels the request; upstream cancellation is not guaranteed.

Results are shown only in the current page and are not stored as agent work.
Tests do not run Goose, validate its sessions/tool loop, or exercise the VM.
Agent tool settings remain immutable. Names, responsibilities and models
can be changed through the [profile editor](agent-profile-editing.md). Settings remains a local single-owner page,
protected by the existing loopback Host/same-origin checks and no-store headers.

Verification: `node --test tests/setup_diagnostics.mjs` uses disposable data and
local synthetic providers, including the browser flow, missing configuration,
consent, cross-origin rejection, concurrent calls, redirects, sanitized errors,
and unchanged agent history. It performs no live paid inference.

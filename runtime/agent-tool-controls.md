# Per-agent browser and memory controls

Open Agents → Details → Edit agent settings. Sourced memory can be enabled on
Goose agents. Browser enablement requires that agent's existing VM manifest and
files to validate with pinned browser version 0.39.1. Missing compatibility is
shown in the editor and rejected by the server without partial changes. This
increment does not install browser components, replace disks or provision VMs.

The profile endpoint accepts optional `memory` and `browser` booleans. Omission
preserves the saved value, or the original configuration if never overridden.
Schema 9 adds nullable flags to each agent's private profile record. Profile
changes and session-map invalidation commit together. Older binaries refuse the
new schema. Tool changes advance the same context generation used by role/model
changes, so the next request uses fresh context without replaying old work.

Edits require no active/queued work, no pending approval and returned browser
control. A browser-operation gate serializes edits against owner takeover and
browser operations. Existing Stop and stale-revision rules remain in effect.
Future requests and scheduled work use the saved switches. Disabling is not a
substitute for Stop; finish or stop current work before editing.

Disabling memory removes its extension, prevents runtime approval and denies
host memory-service calls even with a valid active-run token. No notes or source
records are deleted. The local owner can still read preserved notes via the
memory API; owner correction/forget operations require memory enabled. Disabling
browser removes its extension and prevents runtime approval, guest run arming,
and owner browser operations. Existing browser profiles/files remain. Re-enabling
restores access with fresh model context. These flags do not prevent a model from
using knowledge in its weights or information explicitly supplied by the owner.

Checks are enforced in code, independently of role prompts. Failed reads of saved
flags deny access. The VM memory endpoint may remain mounted for later enablement,
but its active-run and saved-policy checks still apply. Host memory transport only
starts when enabled. Browser manifest compatibility is distinct from runtime
availability: a missing binary or failed VM can still cause an actual request to
fail. No automatic retry or external action replay is introduced.

Agent lists/Details show effective flags. Header tool buttons and open panels
follow saved changes. Model tests remain optional and explicitly consented;
saving tool settings makes no paid inference call. Staff authorization, hosted
tenancy and automatic preparation of incompatible environments remain later work.

Verification: `tests/agent_tools.mjs` covers host memory on/off/restart/re-enable,
retained notes, actual model tool selection and rejected browser enablement. With
`AYATI_VM_TEST_MANIFEST`, its compatible-VM case starts with both tools disabled
and exercises newly enabled browser access. `tests/independent_agents.mjs` covers
owner-control rejection, disabled browser APIs, two-agent isolation, restart,
and restored real browser/memory execution. Rust tests check direct denial of
memory calls and forged disabled-tool approval requests, plus profile persistence.
All provider responses in these checks are synthetic; no paid inference is needed.

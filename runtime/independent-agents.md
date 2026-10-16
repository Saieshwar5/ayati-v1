# Independent local agents

The local owner can create additional lasting roles through **New agent** in the
Agents list (choose Agents in the left rail). Enter a name, responsibility and model supported by the daemon's
configured provider; choose enabled memory/browser tools. Setup runs in the
background and the original chat remains usable. Open the agent when it is ready.

This increment supports four agents total, including the original Ayati agent,
with one preparation at a time. It does not establish a safe four-VM capacity on
all machines. Each agent retains the existing one-active-run queue and VM idle
shutdown policy. Different agents can work independently at the same time.

## Local prerequisites

Start from the repository root after the existing Goose/VM installation and guest
build in README. Set `AYATI_RUNTIME=goose` and `AYATI_EXECUTION=vm`; configure the
provider and the original agent's VM as before. `AYATI_MEMORY=1` and
`AYATI_BROWSER=1` make those options available during creation. Browser support
requires the pinned browser artifacts and a browser-enabled original VM.

The server invokes the existing `tools/prepare-local-vm.py` through system Python,
with a fresh identity, overlay, SSH keys and guest initialization. It reuses pinned
base artifacts; it never clones another agent's workspace or browser profile.
Installation/build prerequisites remain operator steps. Creating the additional
agent after those prerequisites needs no infrastructure commands. Host adapter
and prototype modes cannot create additional agents.

No new dependency, database service, cloud resource or paid model is required for
setup. Model calls still require a configured provider account. Setup does not test
whether that provider supports the entered model; an invalid model can fail at the
first request. Names, responsibilities, models and compatible tool controls use the
[profile editor](agent-profile-editing.md). Deleting
roles, archiving agents and reclaiming failed setup disks are later work.

## State and authority

- The original agent keeps its existing database and directories unchanged.
- `agents.sqlite` stores creation receipts, role/model/tool settings, and setup state.
- Each additional agent has `agents/<UUID>/ayati.sqlite`, its own Goose state and
  working directory, and `vms/<attempt>/vm.json` with a matching guest identity.
- Chat, task receipts, schedules, approvals, action records, memory evidence and
  browser ownership are resolved through that agent's own database/runtime.
  The current storage schema still uses a singleton conversation inside each
  database; this is not a shared multi-tenant database or shared knowledge layer.
- Runtime role instructions accompany each request. They describe responsibility;
  code still limits extensions, validates typed tool calls and requires exact
  owner approval. Role text cannot grant shell, additional tools or foreign memory.
- New agents use the same configured provider account, but their selected models
  and host model gateways are separate. Provider credentials remain on the host.
- The web client reloads when switching agents, closing the old event stream.
  Drafts, pending message receipts and browser controller tokens are keyed by agent.
  An unfinished browser takeover stays paused until explicit handback.

The API selects an agent using `X-Ayati-Agent`; omitting it retains `local-agent`
for existing clients. Event streams and file downloads also accept `?agent=<UUID>`.
Unknown, preparing, failed, duplicate or conflicting selectors fail closed. Other
endpoints reject query selectors. The local owner can access every local agent;
agent selection is not authentication. Same-origin/Host checks still apply. Do not
expose this daemon publicly: staff access, businesses/customer identities and
hosted authentication are not implemented by this increment.

`GET /api/agents` lists profiles, setup state, active/queued counts and pending
approval counts. `POST /api/agents` accepts:

```json
{
  "request_id": "a canonical UUID generated once for this creation",
  "name": "Operations",
  "role": "Prepare daily operations reports.",
  "model": "a model supported by the configured provider",
  "memory": true,
  "browser": true
}
```

The receipt becomes the agent ID. An identical retry returns the same agent;
a changed body with the same receipt is refused. Unknown fields, control characters,
invalid IDs and oversized settings are rejected. A ready profile is exposed only
after the prepared manifest, private directories and runtime have been validated.

## Failure, Stop and recovery

Setup failures preserve their directory and private `setup-logs/<attempt>.log`.
The UI offers **Retry setup**, which creates a fresh attempt; it never overwrites
the failed disk. The matching API is `POST /api/agents/<UUID>/retry` with `{}`.
An interrupted preparation is marked failed after daemon restart and is not
silently retried. Daemon shutdown cancels preparation and all agents; a host
process crash signals the preparation helper to clean up its guest process.

Chat Stop, VM Stop, approvals and browser operations apply to the selected agent.
Stopping the whole daemon still stops every agent. Existing completed history and
memory survive restart. Failed/uncertain work is not automatically replayed.

If the browser controlling tab was lost, the local owner can recover the selected
agent without exposing its saved capability:

```sh
ayati browser release --agent <agent-UUID>
```

Omitting `--agent` retains the original agent behavior. The API's profile ID or
agent link in the UI supplies the UUID.

## Acceptance and remaining work

`tests/independent_agents.mjs` creates a real second guest through Playwright and
uses pinned Goose/MCP/KVM with a scripted provider and synthetic site. It checks
creation deduplication, early/foreign request denial, separate memory and approvals,
combined browser/memory extensions, distinct guest identities/keys, concurrent
work/Stop, per-agent drafts, narrow screens, restart persistence and interrupted
setup/retry. The fixture needs a disposable current browser-enabled VM:

```sh
AYATI_VM_TEST_MANIFEST=/absolute/path/to/vm.json \
  node --test --test-concurrency=1 tests/independent_agents.mjs
```

This is integration evidence, not live-model quality or hosted tenant isolation.
Live-provider memory acceptance still needs locally configured credentials.
The compact Home/Apps/Agents/Settings navigation is implemented; see
[workspace interface](workspace-interface.md). Remaining L6 work includes explicit
staff/business access, structured owner questions, editable role settings, history
pagination and real image/document/audio processing. Shared records and connector
grants remain separate later milestones.

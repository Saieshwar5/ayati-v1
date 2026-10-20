# Authenticated Relay client

`ayati identity` now serves the existing Home, Apps, conversations/agents and
Settings client at `/w/<workspace-id>/`. Sign-in, creation and invitation
acceptance lead to that client. People and My account are embedded in its shared
navigation. The old limited post-login dashboard is removed.

The workspace picker performs a full document navigation: previous subscriptions,
frames and in-memory state are discarded. The last workspace is remembered per
user. Drafts, attachments, pending submissions, browser controller tokens,
schedule/trigger receipts and Home arrangements are namespaced by user and
workspace. Global appearance preference remains device-wide.

## Request boundary

All business API requests, event streams, downloads and generated app documents
use `/w/<id>/api/...`. Every request validates the WorkOS-backed session and the
current local membership before routing to the existing handlers. A workspace ID
or agent ID alone never authorizes access. State and runtime registries are separate
for each workspace, including the default guide whose local ID remains
`local-agent`. Unscoped `/api/...` routes are still denied in identity mode.

Every mutation requires the same-origin Origin and `X-Ayati-Request: identity`.
Existing handler guards still apply. SSE streams recheck local session and
membership at most one second apart and provider status every 15 seconds. Logout,
session expiry and membership loss close streams. The client also rechecks account
and membership periodically and reloads on a change.

The owner can use the existing resource-management APIs within their own workspace.
Admins can manage ordinary members; Members can see People. Admins and Members
receive the same navigation, empty business-resource lists and owner-access
messages. They cannot inherit the original local owner's agent, file, app,
connector or approval authority. Granular human resource sharing and employee
agent dispatch are not implemented in this increment. No shared chat channels
are implied: Chat & agents is the existing agent-conversation interface.

## Storage and execution

Runtimes live under `AYATI_DATA_DIR/identity/workspaces/<id>` with private directory
permissions. They never reuse the legacy database, default VM, proof-tool grants,
browser configuration or external connector account. Existing installation data
is preserved outside this hierarchy; no implicit import into sai-store occurs.

A fresh workspace has empty business data and an unconfigured guide environment.
Its owner can create agents through the existing agent-creation workflow, which
prepares dedicated VM environments under that workspace. The default guide needs
its own VM manifest at `identity/workspaces/<id>/vms/local-agent/vm.json`; the
legacy global `AYATI_VM_MANIFEST` is deliberately ignored. Model provider settings
are host configuration, and model calls can incur the existing provider charges.

External connectors remain disabled in this mode until workspace-specific provider
configuration is implemented. Host-level shutdown is not a workspace API. Existing
Apps and Home panels operate against the workspace's own Platform store. This is
still a local pilot, not a hosted multi-tenant production deployment.

## Verification

- All 32 Rust library tests pass, including 11 identity/gateway tests.
- Tests cover membership and cross-workspace denial, member resource restrictions,
  separate persisted business data, non-reuse of legacy runtime configuration,
  missing sessions, mutation guards and open-stream closure on logout.
- Six client test files pass, including account/workspace draft separation and
  access-loss routing. Client build and browser-test type checking pass.
- Real owner session opened sai-store in the original Home client. Existing Owner
  and Member records appeared in the embedded People screen. Apps and Chat links
  retained workspace scope.
- No model execution, new VM provisioning, external connector operation or shared
  employee resource access was claimed by these checks.

# Connections and delegated setup

The editable OpenPencil concept is [workspace-connections.pen](workspace-connections.pen),
with its source [JSX](workspace-connections.jsx). It explores workspace account
approval separately from agent creation and per-operation delegation.

The connected app saved the document and its node tree was checked. Its raster
export returned `No visible nodes to export`; no concept PNG is claimed. The
screenshots below are from the implemented application, visually inspected after
the local Owner/member/real-Goose journey:

- [Workspace Connections](implemented-desktop.png)
- [Phone delegation drawer](implemented-delegation-mobile.png)
- [Member's agent permissions](implemented-agent-permissions.png)

All identities and business records shown are local simulation data. The current
behavior and remaining limits are in [the runtime guide](../../runtime/workspace-connections.md).
The concept is exploratory; the implemented controls save each operation explicitly.

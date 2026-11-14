# Connected systems

Built 2026-11-14: slice C1 of the owner-approved connected systems plan (D249),
step B1 of agent building blocks (D248). An agent's owner or an admin connects a
company's own MCP server from the agent's **Tools** tab, with no operator file,
reviews each tool, and the agent uses those tools under the rules chosen.
For developers building such a server, see the
[connected systems guide](connected-systems-guide.md).

## What is built

- **Who.** The agent's owner and workspace admins connect, review, switch off
  and remove. Anyone who may see the agent's tools sees its systems' tiles,
  never a key. The server refuses everyone else.
- **Connecting** (`POST …/agent-profiles/{agent}/systems/preview`, then
  `POST …/systems`). Step 1 takes a name, an MCP address and an optional key;
  Cuckoo reaches the address and lists its tools. Nothing is saved. Step 2
  saves the person's review: for each switched-on tool (1 to 20), Reads or
  Changes and Allow / Ask first / Block. Reads start on Allow; changes and
  tools with no hint start on Ask first. On save Cuckoo lists the tools again
  and refuses a tool that changed meanwhile. The browser picks the connection
  id, so a repeat of the same request returns the saved system; the same id
  with a different request is refused.
- **What is saved.** One ordinary `relay_control_connections` row with
  provider `system` and server `system-<connection id>`, an approved
  `relay_control_systems` row, one `relay_connector_rules` row per tool (the
  person's choice; an admin can still make it stricter in Apps), the agent's
  switch (`relay_agent_connections`) and `relay_connected_systems`
  (schema 116): name, agent, the review, who connected it and when.
- **The key.** Sent as a bearer token. It is sealed with
  `AYATI_SECRETS_KEY_FILE` (AES-256-GCM, bound to the workspace and
  connection, so a sealed key moved to another connection can't be opened) and
  kept in the connection's binding with the address. It is opened only to call
  the server, and never reaches the model, logs, Activity or the browser after
  it is entered. Without the secrets file, systems with a key can't be saved.
- **Calls.** A system's tool is a reviewed connector action: Allow / Ask first
  / Block, company rules and budgets apply before anything is sent, approvals
  show the exact inputs and happen inside Cuckoo, calls are recorded and count
  toward the 60-a-minute limit per workspace and app. Each system has its own
  circuit breaker (`sys_<28 characters of its id>`), so one broken system never
  pauses another.
- **Held tools.** Before each call Cuckoo lists the tools and compares the
  tool with what was reviewed; a changed or missing tool is refused (nothing is
  sent) while the others keep working. The nightly drift check records the same
  comparison; the tile then says *Changed since it was reviewed* and names the
  held tools. **Review tools** lists the tools now, with the saved key or a new
  one, and a saved review releases them. Waiting approvals ask again.
- **Tile states.** Connected; Changed since it was reviewed; Cannot reach
  (every tool failed at the last check); Off (switched off: nothing is sent and
  waiting approvals stop). Actions: Review tools, Switch off/on, Remove.
- **Only on its agent.** A system appears only on the Tools tab of agents
  that use it, and only an admin may switch it on for another agent
  (`…/control/tools`).

## Addresses and inputs

- **Addresses.** HTTPS on a public address, no credentials, query or fragment
  in the URL, and no redirects; private, loopback and link-local addresses are
  refused after DNS. Plain HTTP to `127.0.0.1` exists only for local tests.
- **Protocol.** Cuckoo asks for MCP `2025-11-25` and accepts a server that
  answers with `2025-06-18` or `2025-03-26`; every later request names the
  agreed version. (This applies to operator MCP servers too.)
- **Inputs Cuckoo can check.** Up to 20 named fields of text, whole number,
  number, yes/no, one of a fixed list, or a list (at most 20 items) of one of
  those. Details that only narrow a value (`default`, `format`, `minimum`,
  `maxLength`, `pattern`, `examples` and similar) are left out of Cuckoo's
  check; the server still enforces them. An optional value written as
  `["string","null"]` counts as text, and `const` as a one-item list. A nested
  object, `anyOf`/`oneOf`/`allOf`, `$ref` or a list of objects makes the tool
  *Not supported*, with the field or keyword named.
- **Sizes.** 200 tools listed, 20 on; 8 KiB per call, 2,000 bytes per text
  field, 64 KiB per reply; a name of up to 100 characters.

## Not yet

Events from a connected system (C2), results back to it (C4), subjects (C3),
sign-in with the system (OAuth), private addresses through the Cuckoo
Connector, and an admin switch to give a system to another agent from that
agent's Tools tab (today an admin uses `…/control/tools`).

## Checks

Rust: `connectors::system_tools` (schema review), `connectors::systems`
(breaker names, address rules), `connectors::systems_tests` (the provider call
contract against a local StudyHub: read and write run, unconfirmed results are
uncertain, a changed or removed tool is held while others work, a refused key
and a moved sealed key send nothing, error messages say what to fix) and, with
`--include-ignored`, the guide's example server. PostgreSQL
`systems::tests`: saving, exact repeats, rules in the effective view, the key
never stored in plain text, off/on/remove, a stricter review, owner-or-admin
only, and another agent's owner can't take a system. Web
`tests/agent-systems.test.mjs`.

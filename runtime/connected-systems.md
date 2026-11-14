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

## Events (C2) and nested data (C5)

Built 2026-11-14, schema 117. A connected system starts automations by sending
events.

- **Turning events on.** On the system's tile, *Turn on events* (owner or
  admin) makes a key, shown once and kept only as its digest; *Replace the
  key* stops the old one at once; *Turn off events* refuses everything.
- **The call.** `POST /hooks/system/{connection}` with
  `Authorization: Bearer <events key>` and the fixed envelope
  `{"id","type","subject","time","data"}`. `id`: up to 128 letters, digits,
  `-` or `_`, unique within the system. `type`: up to 64 lowercase letters,
  digits, dots or underscores, starting with a letter. `subject` and `time`:
  optional text. `data`: any JSON object. Unknown envelope fields are ignored.
  Up to 64,000 bytes. No cookie authorizes it; it is a narrow exception to the
  browser's Origin checks.
- **Answers.** 200 `{"status":"accepted","matched":n,"id":…}` (saved before the
  answer; `matched` is the number of automations that will run), 200
  `{"status":"duplicate"}` for the same id and content, 401 wrong key or events
  off, 409 the same id with other content, 422 a bad envelope or oversized data
  (the message names the field), 429 too many waiting events.
- **Automations.** *Add automation* on the tile (owner or admin): an event
  type (from those received, or typed), the `data` fields the agent sees (1 to
  8), instructions, Memory and where results go. Each system, type and field
  choice is one `system` event source in `relay_connector_subscriptions`, so
  receipts, duplicate rules, deliveries, dispatch, work rooms, pause, Activity
  and source revocation are the existing ones. A system with no reading tool
  set to Allow can't start work: an event carries what such a read could fetch.
  An event that matches no automation is saved and shown, never matched later.
- **Nested data (C5).** Each chosen field reaches the agent whole, nested
  objects and lists included, with `$type` and `$subject`; together at most
  16 KiB, or the event is refused with the largest field named. Fields not
  chosen never reach the agent. Event contents are data, never instructions.
- **Recent events** (`GET …/systems/{id}/events`): the latest 100 within 30
  days with time, id, type, subject, field names (never values) and how many
  automations ran. *Send test event* sends a sample through the same intake,
  marked as a test.

## Results (C4)

- **Result fields.** An automation may name up to 8, each text, number,
  yes/no or date. The run's request asks the agent to call `report_result`
  once; Cuckoo checks every name and kind and refuses anything else. A run that
  finishes without them counts as failed.
- **Reading.** `GET /hooks/system/{connection}/events/{id}` with the events
  key returns one run per automation: `state` (`queued`, `working`,
  `waiting_for_person`, `done`, `failed`, `cancelled`), the written `answer`
  when done, `result` and `reason`.
- **Stopping.** `POST /hooks/system/{connection}/events/{id}/cancel` stops runs
  that haven't finished. Finished runs keep their result.
- **Not yet:** Cuckoo calling the system back with the result; reading is
  enough for a first version.

## Subjects (C3)

Built 2026-11-14, schema 118. An event's `subject` names one user of the
outside system (a student, a patient, an order). Cuckoo trusts it because it
arrived with the system's key; the system verifies its own users. A subject's
words in `data` are data, and a subject can never approve anything.

- **Per subject memory.** An automation set to *Per subject* runs each event
  in the agent's private room for that system and subject
  (`relay_subject_rooms`), shared by that system's automations, so a later run
  sees only that subject's earlier runs. Events without a subject start fresh.
- **Side by side.** Each delivery keeps its subject. Dispatch keeps one
  unfinished run per automation *and subject*: different subjects run in
  parallel within the queue limits, and one subject's events run in the order
  they arrived. Events without a subject keep the old rule: one run per
  automation.
- **Needs the subject.** When reviewing tools, a tool with a text input named
  `subject` can be marked *Needs the subject*. Cuckoo then hides that input from
  the agent and fills it from the run's event before the action is proposed,
  so approvals and receipts show the exact subject. The agent can't supply one,
  and a run without a subject (a request in Chat) can't use the tool. A system
  that wants staff to look up any student from Chat offers a separate tool with
  an explicit student input and its own rule.
- **Forget this subject** (events dialog): clears that subject's rooms with the
  system's agents (messages and working summary) and its next event starts in a
  new room.
- **Shared places** (notes for every conversation, "always" Library documents
  and the computer's `shared/` folder) reach every run, so the form warns to
  keep nothing about one subject there.
- **Open:** how long a quiet subject's room is kept.

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

The result call back to the system, signed events, declared event types, sign-in with the system (OAuth), private addresses through the Cuckoo
Connector, and an admin switch to give a system to another agent from that
agent's Tools tab (today an admin uses `…/control/tools`).

## Checks

Rust: `connectors::system_tools` (schema review), `connectors::systems`
(breaker names, address rules), `connectors::systems_tests` (the provider call
contract against a local StudyHub: read and write run, unconfirmed results are
uncertain, a changed or removed tool is held while others work, a refused key
and a moved sealed key send nothing, error messages say what to fix) and, with
`--include-ignored`, the guide's example server; `events::system_events`
(envelope, nested projection, size limit) and `events::system_routes`.
PostgreSQL `systems::events_tests`: matched and unmatched events, nested
fields whole and unchosen fields absent, duplicates and conflicts, wrong and
replaced keys, oversized data, owner-only automations and filters, test
events, run states, checked result fields, a run failed without them, and
cancel. `systems::subjects_tests`: a room per subject, subjects side by side
and one subject in order, forgetting a subject, and the subject filled into a
marked tool (hidden from the listing, never from the agent, refused without
one). PostgreSQL
`systems::tests`: saving, exact repeats, rules in the effective view, the key
never stored in plain text, off/on/remove, a stricter review, owner-or-admin
only, and another agent's owner can't take a system. Web
`tests/agent-systems.test.mjs`.

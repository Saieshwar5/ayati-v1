# Communication gateway (schema 54)

One gateway carries messages between agents and the places people write
from. Each live transport is an adapter behind one contract; everything else
is shared. The first live transport is the **Communication MCP**: a small MCP
server a developer writes to bridge a platform (Slack, Teams, Discord, SMS or
their own app). Built 2026-11-01 (milestone G1 of the gateway plan, D236).

Cuckoo Chat keeps its own authenticated path. The local email and WhatsApp
simulations ([native communications](native-communications.md)) are
unchanged and are not adapters.

## How a message travels

**In.** Every few seconds each controller asks every transport server with an
enabled endpoint for events after its saved cursor. For each message:

1. The **endpoint** it was sent to decides the workspace and agent. The message
   never chooses.
2. It is saved once: the same event ID with the same content is a duplicate,
   with different content it is refused.
3. **Who may write:** the sender's platform address must be bound to a
   workspace member who can talk to the agent. Anyone else is held without
   agent work, and their message body is not kept.
4. **Room:** one private conversation per endpoint, sender and platform
   conversation (and thread). Two people never share a room or a context.
5. The message, the request and the receipt are saved together. The cursor
   moves only after the batch is saved, so a crash rereads it and duplicates
   are dropped.

**Out.** When the agent finishes, the reply is saved to the outbox with the
room's recipient. The agent never chooses a recipient. Before sending, Cuckoo
checks again that the request's access, sources and agent still hold and that
the text is unchanged; otherwise the reply is cancelled. Then:

| What the platform said | Saved state | What happens next |
| --- | --- | --- |
| Took it | `accepted` | A receipt can mark it `delivered` |
| Delivered it | `delivered` | Done |
| Said no | `failed`, with its reason | Nothing more |
| Server unreachable | `queued` again | Tried again after 30 seconds; the attempt does not count |
| Unclear (timeout, lost answer, crash mid-send) | `uncertain` | Never resent. Cuckoo asks `message_status` every minute until the platform knows |

A receipt never downgrades a delivered message. Switching an endpoint off
stops reading it and cancels replies not yet sent.

Agents keep the external manners of schema 52: one complete reply, no
"working on it" updates and no front desk.

## The adapter contract

`src/identity/communications/adapters.rs` defines `TransportAdapter`:

| Method | Purpose |
| --- | --- |
| `servers` | The transport servers it bridges for a workspace |
| `check` | Confirm an address is served before an endpoint is saved |
| `fetch` | Events after a cursor; malformed and group events are dropped and counted |
| `send` | Send with an idempotency key: `Accepted`, `Delivered`, `Refused`, `Unreached` (never reached) or `Uncertain` |
| `status` | What became of an unclear send |

Adding a transport is one module implementing it, one line in
`adapters::configure`, and passing `communications/conformance_tests.rs`.
That suite covers checking an address, reading with a cursor, dropping group
and malformed events, ignoring display names, sending exactly what Cuckoo
chose, receipts, refusals, lost answers settled by asking, and an unreachable
platform. Pull adapters fit platforms that can be polled (Telegram's
`getUpdates`, mailbox history). Webhook ingress is not built yet.

## Communication MCP, protocol v1

A transport server is a remote MCP server (Streamable HTTP, protocol
`2025-11-25`). It declares
`capabilities.experimental["cuckoo/transport"] = {"version": 1}` and
`capabilities.tools`. Every tool returns `structuredContent`. Unknown fields
are refused.

| Tool | Arguments | Result |
| --- | --- | --- |
| `transport_info` | none | `{name, version}` |
| `list_endpoints` | none | `{endpoints: [{address, name?}]}`, at most 100 |
| `fetch_inbound` | `{after, limit}` (cursor or null, 20) | `{events: [...], next}`. Keep events until Cuckoo asks with a later cursor. |
| `send_message` | `{key, endpoint, to, conversation, thread, text}` | `{state, provider_id?, reason?}`, where `state` is `accepted`, `delivered`, `refused` or `uncertain`. Never send twice for one `key`. An error result means refused. |
| `message_status` | `{key}` | The same shape; `unknown` while the platform has no record |

`text` is Markdown; the server converts or strips formatting and splits long
messages for its platform.

A message event:

```json
{"type": "message", "id": "evt-1842", "endpoint": "nova-bot",
 "sender": {"address": "U123", "name": "Ravi"},
 "conversation": {"id": "D456", "kind": "direct", "thread": null},
 "text": "What is my leave balance?", "automated": false, "at": "2026-11-01T10:00:00Z"}
```

A receipt event: `{"type": "receipt", "id", "key", "state"}`, where `state` is
`delivered`, `read` or `failed`. Identifiers are at most 320 characters and
text at most 12,000 bytes.

Rules Cuckoo enforces on every transport server:

- **It serves one workspace**, named in the operator file. An event for an
  address with no endpoint is dropped.
- **Display names are not identity.** Who a sender is comes from Cuckoo's
  bindings.
- **Only direct conversations in v1.** In a group, being in the group is no
  permission, so group events are dropped.
- **The recipient comes from the room.** The server must send only there.
- **The server gets only the message to send.** It never sees context, notes,
  tool results or Cuckoo's credentials. Cuckoo never holds the platform's
  credentials.
- **A transport message never approves an action.**

## Setup

1. Write the operator file, an owned 0600 file named by
   `AYATI_TRANSPORT_SERVERS_FILE` on the controller (executors refuse it). It is
   read on every use, so servers and tokens can change without a restart:

   ```json
   [{"id": "team-chat", "workspace": "<workspace id>", "name": "Team chat",
     "url": "https://bridge.example.com/mcp", "bearer_file": "/absolute/private/team-chat-token"}]
   ```

   HTTPS is required; plain HTTP only on `127.0.0.1` for local tests. The
   address must resolve to a public IP, as for connector MCP servers.
2. An admin adds the agent's address (the server must list it):
   `POST /api/identity/workspaces/{w}/communications/endpoints` with
   `{id, agent, transport: "mcp", server, address}`.
3. An admin binds each platform user to a member:
   `POST …/communications/endpoints/{id}/bindings` with `{sender, user}`. This
   is the admin's own statement, not a verification. Pairing codes and
   one-time codes come with contacts (G3).
4. Switch an endpoint or a binding off or on:
   `POST …/endpoints/{id}/state` and `POST …/endpoints/{id}/bindings/{sender}/state`
   with `{revision, enabled}`.

`GET …/communications` also lists the transports and servers offered to the
workspace. The simulation API cannot change live endpoints, and the live API
cannot change simulations. There is no screen yet; the agent's Communication
tab needs an approved design.

## Schema 54

- `relay_communication_endpoints`: `transport` is any adapter name, `server`
  names the transport server, and `simulated` is false for live endpoints. An
  address is unique per transport, or per workspace and server.
- `relay_communication_outbox.reason`: why a send failed or is unclear.
- `relay_communication_cursors`: how far each server has been read.

## Checks

- Unit: `communications::conformance_tests` (the adapter suite against a
  local server, other protocol versions, unconfigured servers).
- PostgreSQL: `communications::gateway_tests`. They cover reading, held
  strangers, duplicates and changed retries, cursors, sending to the room's
  recipient, receipts, refusals, outages, unclear sends settled without
  resending, a crash mid-send, and switching an endpoint off.
- Journey: `tests/communication_gateway.mjs`, with real Goose and a local
  transport server. Two people get separate rooms and answers, a stranger and
  a group message get nothing, the second person's model input never contains
  the first person's private text, a lost answer is settled after a restart
  without resending, switching the endpoint off stops reading, and the token
  never reaches the model, logs or database.

## Not built yet

- **Shared context for Chat channels** (G2).
- **Contacts and verification** (G3): pairing, one-time codes, assurance levels.
- **Knowledge audiences and actions tied to the asker** (G4). No public
  endpoint should open before G3 and G4.
- **Native email, Telegram, WhatsApp, website chat and calls.**
- **Webhook ingress.**
- **For developers:** a doorbell notification, attachments, a TypeScript
  template and `ayati transport check`.
- **Group conversations.**
- **Settling unclear sends** is retried every minute without a limit.

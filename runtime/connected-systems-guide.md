# Connecting your own system to a Cuckoo agent

This guide is for developers. Your company built its own software (an order
system, a school app, a clinic system) and wants a Cuckoo agent to read from it
and act in it. You add one small **MCP server** to your system; the agent's
owner then connects it from the agent's **Tools** tab. Nothing in Cuckoo has to
be installed or configured by an operator.

The example below is **StudyHub**, a tutoring school's app.
[`tools/examples/studyhub_mcp.py`](../tools/examples/studyhub_mcp.py) is a
complete server in about 120 lines of standard-library Python.

## 1. What you build

An MCP server is one HTTPS address that answers two questions: *which tools do
you have?* (`tools/list`) and *run this tool* (`tools/call`). It speaks
[Streamable HTTP MCP](https://modelcontextprotocol.io/specification/2025-06-18/basic/transports):
JSON-RPC 2.0 in a `POST`, answered with JSON (or an event stream).

| Request | Your server answers |
| --- | --- |
| `initialize` | `{"protocolVersion":"2025-06-18","capabilities":{"tools":{}},"serverInfo":{…}}` |
| `notifications/initialized` (no `id`) | HTTP 202, no body |
| `tools/list` | `{"tools":[…]}`, at most 200, no paging |
| `tools/call` | `{"content":[{"type":"text","text":"…"}],"structuredContent":{…}}`, or `"isError":true` |

Cuckoo asks for version `2025-11-25` and accepts `2025-06-18` or `2025-03-26`.

## 2. Describe each tool

```json
{
  "name": "get_marks",
  "title": "Read exam marks",
  "description": "A student's latest exam marks by subject.",
  "annotations": {"readOnlyHint": true},
  "inputSchema": {
    "type": "object",
    "properties": {"student": {"type": "string", "description": "The student's id"}},
    "required": ["student"]
  }
}
```

- **One tool per action**, with a plain name (letters, digits, `_`, `-`, `.`)
  and a title and description a shop owner understands.
- **Say whether it changes anything.** `readOnlyHint: true` makes Cuckoo
  suggest *Reads* and *Allow*; anything else starts as *Changes* and *Ask
  first*. The person connecting decides.
- **Flat inputs.** Up to 20 fields of text, number, whole number, yes/no, one
  of a fixed list (`enum`), or a list of one of those. Cuckoo leaves checks such
  as `minLength`, `maximum`, `pattern` or `format` to you; it never relaxes them.
  A nested object or `anyOf`/`oneOf` makes the tool *Not supported* in the
  review screen, which names the field. Split such a tool into simpler ones.
- **Return only what the call needs.** Replies are limited to 64 KiB. Every
  input is at most 8 KiB in all and 2,000 bytes per text field.

## 3. Protect it

- Serve it over **HTTPS on a public address**. Cuckoo refuses private and
  loopback addresses, URLs with credentials or a query, and redirects.
- Require a **long random key** and compare it in constant time. Cuckoo sends
  it as `Authorization: Bearer <key>`, stores it sealed and never shows it
  again. To change it, give the owner the new key; they enter it under
  **Review tools**.
- **Don't log request bodies**: they carry your users' data.
- Cuckoo calls a tool only under the rule its owner chose. Changes can be set
  to *Ask first*, and then a person approves the exact inputs inside Cuckoo
  before your server sees the call. A key never approves anything.

## 4. Connect it

1. Open the agent, then **Tools**, then **Connect a system**.
2. Enter a name, the MCP address and the key. Choose **Find its tools**.
3. Review each tool: switch it on, choose Reads or Changes, and Allow, Ask
   first or Block. Choose **Connect**.

Only the agent's owner and workspace admins can do this.

## 5. Change it later

- **Changing a tool's inputs** or removing it holds that tool: Cuckoo refuses
  to call it until the owner reviews it again, while the other tools keep
  working. The tile says *Changed since it was reviewed*.
- **Adding a tool** does nothing until the owner switches it on in a review.
- **Switch off** stops every call at once; **Remove** takes the system away.

## 6. Try the example

```sh
STUDYHUB_KEY=choose-a-long-random-key python3 tools/examples/studyhub_mcp.py
```

It listens on `127.0.0.1:8765/mcp`. Put an HTTPS proxy in front of it (any
public HTTPS address works), then connect that address. Cuckoo's own client and
reviewer run against the example in the test
`identity::connectors::systems_tests::the_guide_example_server_connects_and_every_tool_is_usable`.

## What comes next

Your system will also be able to **start** the agent by sending an event, and
to read the result back. Those steps extend this same connection.

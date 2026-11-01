import { createServer } from 'node:http';
import { once } from 'node:events';

// A local Communication MCP server (protocol v1): a stand-in for a developer's
// bridge to some messaging platform. Events wait in a log read with a cursor;
// sends are recorded, and the same key never sends twice.
export async function transportServer() {
  const token = 'PRIVATE_TRANSPORT_FIXTURE_TOKEN';
  const events = [], sent = [], delivered = new Map();
  let mode = 'accept';
  const structured = value => ({ content: [{ type: 'text', text: JSON.stringify(value) }], structuredContent: value });
  const tools = {
    transport_info: () => structured({ name: 'Team chat fixture', version: '1' }),
    list_endpoints: () => structured({ endpoints: [{ address: 'nova-bot', name: 'Nova' }] }),
    fetch_inbound: ({ after, limit }) => {
      const start = after ? Number(after) : 0;
      const page = events.slice(start, start + limit);
      return structured({ events: page, next: page.length ? String(start + page.length) : after ?? null });
    },
    send_message: args => {
      if (mode === 'refuse') return { isError: true, content: [{ type: 'text', text: 'Blocked by the platform' }] };
      if (!delivered.has(args.key)) { sent.push(args); delivered.set(args.key, 'delivered'); }
      if (mode === 'lose') return undefined;
      return structured({ state: 'accepted', provider_id: `m-${sent.length}` });
    },
    message_status: ({ key }) => structured({ state: delivered.get(key) ?? 'unknown' }),
  };
  const server = createServer(async (request, response) => {
    if (request.method !== 'POST' || request.headers.authorization !== `Bearer ${token}`) { response.writeHead(403).end(); return; }
    let body = ''; for await (const bytes of request) body += bytes;
    const message = JSON.parse(body);
    const reply = result => { response.setHeader('Content-Type', 'application/json'); response.end(JSON.stringify({ jsonrpc: '2.0', id: message.id, result })); };
    if (message.method === 'initialize') {
      reply({ protocolVersion: '2025-11-25', capabilities: { tools: {}, experimental: { 'cuckoo/transport': { version: 1 } } }, serverInfo: { name: 'transport-fixture', version: '1' } });
      return;
    }
    if (message.method === 'notifications/initialized') { response.writeHead(202).end(); return; }
    const tool = message.method === 'tools/call' && tools[message.params.name];
    if (!tool) { response.writeHead(400).end(); return; }
    const result = tool(message.params.arguments ?? {});
    // The platform sent it, but the answer never arrives.
    if (result === undefined) { response.writeHead(500).end(); return; }
    reply(result);
  });
  server.listen(0, '127.0.0.1'); await once(server, 'listening');
  return {
    token, url: `http://127.0.0.1:${server.address().port}/mcp`, events, sent,
    message: (id, sender, text, kind = 'direct') => events.push({ type: 'message', id, endpoint: 'nova-bot',
      sender: { address: sender, name: 'Claimed Owner' }, conversation: { id: `D-${sender}`, kind }, text }),
    setMode: value => { mode = value; },
    close: () => new Promise(resolve => { server.close(resolve); server.closeAllConnections(); }),
  };
}

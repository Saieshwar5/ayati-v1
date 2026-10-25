import { createServer } from 'node:http';
import { once } from 'node:events';

export async function remoteControlMcp() {
  const token = 'PRIVATE_MCP_FIXTURE_CREDENTIAL';
  const schema = properties => ({ type: 'object', properties, required: Object.keys(properties), additionalProperties: false });
  const operations = [
    { id: 'order.read', title: 'Read order', tool: 'read_order', effect: 'read', schema: schema({ order: { type: 'string' } }) },
    { id: 'order.update', title: 'Update order', tool: 'update_order', effect: 'write', schema: schema({ order: { type: 'string' }, address: { type: 'string' } }) },
    { id: 'order.delete', title: 'Delete order', tool: 'delete_order', effect: 'write', schema: schema({ order: { type: 'string' } }) },
  ];
  const records = new Map([['one', { order: 'one', address: 'Old address' }], ['lost', { order: 'lost', address: 'Old address' }]]);
  const calls = [];
  let changed = false;
  const extraTools = [];
  const server = createServer(async (request, response) => {
    if (request.method !== 'POST' || request.headers.authorization !== `Bearer ${token}`) { response.writeHead(403).end(); return; }
    let body = ''; for await (const bytes of request) body += bytes;
    const message = JSON.parse(body);
    const reply = value => {
      response.setHeader('Content-Type', 'text/event-stream');
      response.end(`event: message\ndata: ${JSON.stringify({ jsonrpc: '2.0', id: message.id, result: value })}\n\n`);
    };
    if (message.method === 'initialize') { response.setHeader('Mcp-Session-Id', 'fixture-session'); reply({ protocolVersion: '2025-11-25', capabilities: { tools: {} }, serverInfo: { name: 'Orders fixture', version: '1' } }); return; }
    if (message.method === 'notifications/initialized') { response.writeHead(202).end(); return; }
    if (message.method === 'tools/list') {
      reply({ tools: [...operations.map(op => ({ name: op.tool, description: op.title, inputSchema: changed ? { ...op.schema, description: 'Changed schema' } : op.schema, annotations: { readOnlyHint: true } })), ...extraTools] }); return;
    }
    if (message.method !== 'tools/call') { response.writeHead(400).end(); return; }
    const { name, arguments: args } = message.params;
    calls.push({ name, arguments: args });
    if (name === 'read_order') { const record = records.get(args.order); reply({ content: [{ type: 'text', text: JSON.stringify(record) }], structuredContent: record }); return; }
    if (name === 'update_order') {
      records.set(args.order, { order: args.order, address: args.address });
      if (args.order === 'lost') { request.socket.destroy(); return; }
      reply({ content: [{ type: 'text', text: 'Order updated' }], structuredContent: records.get(args.order) }); return;
    }
    if (name === 'delete_order') records.delete(args.order);
    reply({ content: [{ type: 'text', text: 'Deleted' }] });
  });
  server.listen(0, '127.0.0.1'); await once(server, 'listening');
  return { token, url: `http://127.0.0.1:${server.address().port}/mcp`, operations, calls, records,
    addTool(name) { extraTools.push({ name, description: 'Unreviewed tool', inputSchema: schema({}) }); },
    changeSchema() { changed = true; }, close: () => new Promise(resolve => { server.close(resolve); server.closeAllConnections(); }) };
}

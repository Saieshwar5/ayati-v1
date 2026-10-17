import { createServer } from 'node:http';
import { once } from 'node:events';

// Wire shapes checked against Composio Connect on 2026-10-17. No live credentials.
export async function composioMcpFixture() {
  const calls = [];
  let active = true, changed = false, loseReply = false;
  const server = createServer(async (request, response) => {
    let raw = ''; for await (const chunk of request) raw += chunk;
    const body = JSON.parse(raw); calls.push(body);
    if (request.headers['x-consumer-api-key'] !== 'ck_fixture' || request.headers['x-api-key']) { response.writeHead(401).end(); return; }
    if (body.method === 'notifications/initialized') { response.writeHead(202).end(); return; }
    const schema = { toolkit: 'GMAIL', tool_slug: 'GMAIL_FETCH_EMAILS', description: changed ? 'Changed schema' : 'Read inbox', input_schema: { type: 'object', properties: { max_results: { type: 'integer' } } } };
    let result;
    if (body.method === 'initialize') {
      response.setHeader('Mcp-Session-Id', 'fixture-session');
      result = { protocolVersion: '2024-11-05', capabilities: { tools: {} }, serverInfo: { name: 'fixture', version: '1' } };
    } else {
      if (request.headers['mcp-session-id'] !== 'fixture-session') { response.writeHead(400).end(); return; }
      const { name, arguments: args } = body.params;
      let data;
      if (name === 'COMPOSIO_SEARCH_TOOLS') data = { tool_schemas: { GMAIL_FETCH_EMAILS: schema }, toolkit_connection_statuses: [{ toolkit: 'gmail', has_active_connection: active }], session: { id: 'discovery' } };
      else if (name === 'COMPOSIO_GET_TOOL_SCHEMAS') data = { success: true, tool_schemas: { GMAIL_FETCH_EMAILS: schema } };
      else if (name === 'COMPOSIO_MANAGE_CONNECTIONS') {
        const adding = args.toolkits[0].action === 'add';
        data = { results: { gmail: adding ? { account_id: 'ca_added', redirect_url: 'https://connect.composio.dev/fixture', link_token: 'DO_NOT_EXPOSE_LINK' } : { accounts: [{ id: 'ca_consumer', status: active ? 'active' : 'expired', user_info: { email: 'DO_NOT_EXPOSE_IDENTITY', token: 'DO_NOT_EXPOSE_TOKEN' } }] } } };
      } else if (name === 'COMPOSIO_MULTI_EXECUTE_TOOL') {
        if (loseReply) { request.socket.destroy(); return; }
        data = { results: [{ tool_slug: args.tools[0].tool_slug, index: 0, response: { successful: true, data: { messages: [{ subject: 'Synthetic consumer inbox' }] } } }], success_count: 1, error_count: 0 };
      } else { response.writeHead(400).end(); return; }
      result = { isError: false, content: [{ type: 'text', text: JSON.stringify({ successful: true, data, error: null, log_id: 'log_mcp' }) }] };
    }
    response.setHeader('Content-Type', 'text/event-stream');
    const event = `: connected\r\n\r\nevent: message\r\ndata: ${JSON.stringify({ jsonrpc: '2.0', id: body.id, result })}\r\n\r\n`;
    response.write(event.slice(0, 50)); response.end(event.slice(50));
  });
  server.listen(0, '127.0.0.1'); await once(server, 'listening');
  return { url: `http://127.0.0.1:${server.address().port}/api/v3.1/`, calls,
    active: value => { active = value; }, changed: value => { changed = value; }, loseReply: value => { loseReply = value; },
    close: () => new Promise(resolve => { server.close(resolve); server.closeAllConnections(); }) };
}

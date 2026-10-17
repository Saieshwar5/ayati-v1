import { createServer } from 'node:http';
import { once } from 'node:events';

// Synthetic Composio wire contract. Never loads account credentials or calls Google.
export async function composioFixture() {
  const calls = [];
  let user, active = true, loseReply = false, foreign = false;
  const server = createServer(async (request, response) => {
    let raw = ''; for await (const chunk of request) raw += chunk;
    const body = raw ? JSON.parse(raw) : undefined;
    calls.push({ path: request.url, method: request.method, body });
    if (request.headers['x-api-key'] !== 'fixture-composio-key') { response.writeHead(401).end('{}'); return; }
    const path = request.url.split('?')[0];
    let data;
    if (path === '/api/v3.1/auth_configs') data = { items: [{ id: 'ac_fixture', name: 'Gmail fixture', toolkit: { slug: 'gmail' }, credentials: { secret: 'DO_NOT_EXPOSE_AUTH_SECRET' } }] };
    else if (path === '/api/v3.1/connected_accounts/link') {
      user = body.user_id; data = { connected_account_id: 'ca_fixture', redirect_url: 'https://connect.composio.dev/fixture', link_token: 'DO_NOT_EXPOSE_LINK_TOKEN' };
    } else if (path === '/api/v3.1/connected_accounts/ca_fixture') {
      data = { id: 'ca_fixture', user_id: foreign ? 'foreign-workspace' : user, toolkit: { slug: 'gmail' }, status: active ? 'ACTIVE' : 'EXPIRED', state: { val: { access_token: 'DO_NOT_EXPOSE_GOOGLE_TOKEN' } } };
    } else if (path === '/api/v3.1/trigger_instances/active') {
      data = { items: [{ id: 'ti_fixture', connected_account_id: 'ca_fixture', user_id: user, trigger_name: 'GMAIL_NEW_GMAIL_MESSAGE', version: '20261001_00' }] };
    } else if (path.endsWith('/upsert')) {
      data = { trigger_id: 'ti_fixture' };
    } else if (path.includes('/triggers_types')) {
      const type = { slug: 'GMAIL_NEW_GMAIL_MESSAGE', name: 'New email', toolkit: { slug: 'gmail' }, description: 'Synthetic new Gmail event', version: '20261001_00', config: {} };
      data = path.endsWith('/triggers_types') ? { items: [type] } : type;
    } else if (path.startsWith('/api/v3.1/tools/execute/')) {
      if (loseReply) { request.socket.destroy(); return; }
      data = { successful: true, error: null, data: { messages: [{ id: 'synthetic-message', subject: 'Synthetic stock update' }] }, log_id: 'log_fixture' };
    } else {
      const tool = { slug: 'GMAIL_FETCH_EMAILS', name: 'Read Gmail', toolkit: { slug: 'gmail' }, description: 'Read a bounded inbox page', version: '20261001_00', input_parameters: { type: 'object', properties: { max_results: { type: 'integer' } } } };
      data = path === '/api/v3.1/tools' ? { items: [tool], next_cursor: null } : tool;
    }
    response.setHeader('Content-Type', 'application/json'); response.end(JSON.stringify(data));
  });
  server.listen(0, '127.0.0.1'); await once(server, 'listening');
  return { url: `http://127.0.0.1:${server.address().port}/api/v3.1/`, calls,
    active: value => { active = value; }, loseReply: value => { loseReply = value; }, foreign: value => { foreign = value; },
    close: () => new Promise(resolve => { server.close(resolve); server.closeAllConnections(); }) };
}

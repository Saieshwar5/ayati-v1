import { createServer } from 'node:http';
import { once } from 'node:events';
import { randomUUID } from 'node:crypto';

// A local stand-in for Composio's v3.1 API and its hosted sign-in page, shaped
// like the documented answers. Account answers carry a token on purpose:
// Cuckoo must never store, log or show it.
export const COMPOSIO_TOKEN = 'DO_NOT_EXPOSE_GOOGLE_TOKEN';
export const COMPOSIO_KEY = 'ak_fixture_project_key';
const VERSION = '20260901_00';
const text = description => ({ type: 'string', description });
const tools = {
  GMAIL_FETCH_EMAILS: { type: 'object', properties: { query: text('Gmail search'), max_results: { type: 'integer' }, page_token: { type: 'string' }, label_ids: { type: 'array' }, user_id: { type: 'string' } }, required: [] },
  GMAIL_SEND_EMAIL: { type: 'object', properties: { recipient_email: text('Who gets the email'), subject: { type: 'string' }, body: { type: 'string' }, is_html: { type: 'boolean' }, cc: { type: 'array' }, user_id: { type: 'string' } }, required: [] },
  GMAIL_CREATE_EMAIL_DRAFT: { type: 'object', properties: { recipient_email: { type: 'string' }, subject: { type: 'string' }, body: { type: 'string' }, is_html: { type: 'boolean' }, thread_id: { type: 'string' } }, required: [] },
  GMAIL_REPLY_TO_THREAD: { type: 'object', properties: { thread_id: { type: 'string' }, message_body: { type: 'string' }, recipient_email: { type: 'string' }, is_html: { type: 'boolean' } }, required: ['thread_id'] },
};

export async function composioProvider() {
  const accounts = new Map(), configs = [], executed = [], removed = [];
  let origin;
  const server = createServer(async (request, response) => {
    let raw = ''; for await (const chunk of request) raw += chunk;
    const body = raw ? JSON.parse(raw) : undefined;
    const url = new URL(request.url, origin), path = url.pathname;
    const send = (data, status = 200) => { response.writeHead(status, { 'Content-Type': 'application/json' }); response.end(JSON.stringify(data)); };
    // The hosted sign-in page: the person signs in, then goes back to Cuckoo.
    if (path.startsWith('/sign-in/')) {
      const account = accounts.get(path.slice(9));
      if (!account) return send({}, 404);
      account.status = 'ACTIVE';
      response.writeHead(302, { Location: `${account.callback}?status=success&connected_account_id=${account.id}` }); response.end(); return;
    }
    if (request.headers['x-api-key'] !== COMPOSIO_KEY) return send({ error: { message: 'bad key' } }, 401);
    const api = path.replace('/api/v3.1/', '');
    if (api === 'auth_configs' && request.method === 'GET') {
      return send({ items: configs.filter(c => c.toolkit === url.searchParams.get('toolkit_slug')).map(c => ({ id: c.id, name: 'Managed', toolkit: { slug: c.toolkit }, status: 'ENABLED', is_composio_managed: true, no_of_connections: 0 })), next_cursor: null });
    }
    if (api === 'auth_configs') {
      const config = { id: `ac_${randomUUID().replaceAll('-', '')}`, toolkit: body.toolkit.slug }; configs.push(config);
      return send({ toolkit: { slug: config.toolkit }, auth_config: { id: config.id, auth_scheme: 'OAUTH2', is_composio_managed: true } });
    }
    if (api === 'connected_accounts/link') {
      const config = configs.find(c => c.id === body.auth_config_id);
      const account = { id: `ca_${randomUUID().replaceAll('-', '')}`, toolkit: config.toolkit, config: config.id, status: 'INITIATED', callback: body.callback_url, user: body.user_id };
      accounts.set(account.id, account);
      return send({ link_token: 'lt_fixture', redirect_url: `${origin}/sign-in/${account.id}`, expires_at: '2026-11-01T00:10:00Z', connected_account_id: account.id });
    }
    const accountPath = api.match(/^connected_accounts\/([a-z0-9_]+)(\/revoke)?$/);
    if (accountPath) {
      const account = accounts.get(accountPath[1]);
      if (!account) return send({}, 404);
      if (accountPath[2]) { account.status = 'REVOKED'; return send({ success: true }); }
      if (request.method === 'DELETE') { accounts.delete(account.id); removed.push(account.id); return send({ success: true }); }
      return send({ id: account.id, status: account.status, toolkit: { slug: account.toolkit }, auth_config: { id: account.config, is_disabled: false }, is_disabled: false, alias: null, state: { authScheme: 'OAUTH2', val: { access_token: COMPOSIO_TOKEN } }, params: {} });
    }
    if (api.startsWith('tools/execute/')) {
      const account = accounts.get(body.connected_account_id);
      if (account?.status !== 'ACTIVE' || body.version !== VERSION) return send({ data: {}, error: 'not active', successful: false });
      executed.push({ tool: api.slice(14), arguments: body.arguments });
      return send({ data: { id: `msg_${executed.length}`, sent_to: body.arguments.recipient_email }, error: null, successful: true, log_id: 'log_fixture' });
    }
    if (api.startsWith('tools/')) {
      const slug = api.slice(6), schema = tools[slug];
      if (!schema) return send({}, 404);
      return send({ slug, name: slug, description: '', toolkit: { slug: 'gmail', name: 'Gmail', logo: '' }, input_parameters: schema, output_parameters: {}, no_auth: false, available_versions: [VERSION], version: VERSION, scopes: [], scope_requirements: null, tags: [], is_deprecated: false });
    }
    send({}, 404);
  });
  server.listen(0, '127.0.0.1'); await once(server, 'listening');
  origin = `http://127.0.0.1:${server.address().port}`;
  return {
    url: `${origin}/api/v3.1/`, accounts, executed, removed,
    expire: () => { for (const account of accounts.values()) account.status = 'EXPIRED'; },
    close: () => new Promise(resolve => { server.close(resolve); server.closeAllConnections(); }),
  };
}

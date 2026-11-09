import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

const w = '11111111-1111-4111-8111-111111111111';
const base = `http://localhost/w/${w}/?view=apps`;
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const session = role => response({ user: { id: 'me', email: 'me@test' }, workspaces: [{ id: w, name: 'Mehta', role }] });
const zoho = { id: 'c1', server: 'zoho', name: 'Zoho Books', kind: 'service', provider: 'composio', account: 'accounts@mehta.test', status: 'connected', rules: 2,
  actions: [{ id: 'invoice.read', title: 'Read invoices', meaning: 'read', mode: 'allow' }, { id: 'invoice.create', title: 'Create an invoice', meaning: 'money', mode: 'ask' }, { id: 'invoice.delete', title: 'Delete an invoice', meaning: 'delete', mode: 'block' }],
  agents: [{ id: 'a1', name: 'Atlas', avatar: 'robot-01' }, { id: 'a2', name: 'Ledger', avatar: 'robot-05' }] };
const gmail = { id: 'c2', server: 'gmail', name: 'Gmail', kind: 'service', provider: 'composio', account: null, status: 'reconnect', rules: 0, actions: [{ id: 'mail.send', title: 'Send email', meaning: 'sends_outside', mode: 'ask' }], agents: [] };

test('Members see every connected app, what agents may do and who uses it, read-only', async () => {
  await reactFixture(async ({ render, click }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { AppsPage } = await import('../src/react/apps/apps-page.tsx');
    await render(AppsPage, { href: base });
    assert.deepEqual([...document.querySelectorAll('.app-name')].map(n => n.textContent), ['Zoho Books', 'Gmail']);
    const panel = document.querySelector('.app-panel');
    assert.match(panel.textContent, /Zoho Books.*Read invoices.*Reads.*Allow.*Create an invoice.*Money.*Ask first.*Delete an invoice.*Deletes.*Block/);
    assert.match(panel.textContent, /Used by.*Atlas and Ledger/);
    assert.match(panel.textContent, /2 company rules/);
    assert.equal(panel.querySelector('.mode-picker, .service-kind-button'), null, 'rules are read, not changed');
    assert.equal([...document.querySelectorAll('button')].some(b => ['Add app', 'Manage', 'Reconnect'].includes(b.textContent) || b.textContent.startsWith('Disconnect')), false);
    // The "need you" pill shows only apps with a problem.
    await click(document.querySelector('.apps-needs'));
    assert.deepEqual([...document.querySelectorAll('.app-name')].map(n => n.textContent), ['Gmail']);
    assert.match(document.querySelector('.app-panel').textContent, /until an admin reconnects it/);
  }, { url: base, fetch: async path => path.endsWith('/session') ? session('Member') : response({ apps: [{ ...zoho, account: null }, gmail], systems: [], can_manage: false, requests_waiting: 0 }) });
});

const catalogPage = { categories: [{ id: 'email_calendar', name: 'Email and calendar', count: 2 }], can_manage: true, next_cursor: null,
  apps: [{ id: 'gmail', name: 'Gmail', description: 'Email', category: 'email_calendar', kinds: ['read', 'sends_outside'], actions: 4, state: 'connected', connection: 'c2' },
    { id: 'outlook', name: 'Outlook', description: 'Email and calendar', category: 'email_calendar', kinds: ['read', 'change'], actions: 6, state: 'available', connection: null }] };

test('Old Connections links open Apps on the app that needs attention; Add app opens the catalog', async () => {
  await reactFixture(async ({ render, click, button }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { AppsPage } = await import('../src/react/apps/apps-page.tsx');
    await render(AppsPage, { href: `http://localhost/w/${w}/?view=settings&section=connections` });
    assert.equal(document.querySelector('dialog[open]'), null, 'no drawer any more');
    assert.equal(document.querySelector('.app-panel h2').textContent, 'Gmail', 'the app that needs reconnecting is shown first');
    assert.match(document.querySelector('.app-line').textContent, /accounts@mehta\.test/);
    await click(button('Add app'));
    const dialog = document.querySelector('dialog[open]');
    assert.equal(dialog.querySelector('h2').textContent, 'Add app');
    assert.deepEqual([...dialog.querySelectorAll('.catalog-tile strong')].map(n => n.textContent), ['Gmail', 'Outlook']);
    assert.match(dialog.querySelector('.catalog-nav').textContent, /MCP serverNext/);
    assert.equal([...dialog.querySelectorAll('button')].some(b => b.textContent.includes('MCP server')), false, 'MCP servers come next and cannot be chosen yet');
    // A connected tile opens its panel.
    await click(dialog.querySelector('[aria-label="Gmail is connected. Open its panel"]'));
    assert.equal(document.querySelector('dialog[open]'), null);
    assert.equal(document.querySelector('.app-panel h2').textContent, 'Gmail');
  }, { url: `http://localhost/w/${w}/?view=apps`, fetch: async path => path.endsWith('/session') ? session('Admin')
    : path.endsWith('/control/connections') ? response({ servers: [], systems: [], connections: [], safety: [], people: [], viewer: 'me', execution_available: true })
    : path.includes('/apps/catalog') ? response(catalogPage)
    : response({ apps: [zoho, gmail], systems: [], can_manage: true, requests_waiting: 0 }) });
});

const portal = { id: 'p1', kind: 'link', name: 'Order portal', description: 'Orders and deliveries', audience: 'selected', revision: 2, can_open: false, url: null, owner: 'o', owner_name: 'Ravi', can_decide: false, people: null, requested: false };

test('A member sees a company app they cannot open, asks once for access and can suggest an app', async () => {
  const sent = [];
  let requested = false;
  await reactFixture(async ({ render, click, change, update, button }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { AppsPage } = await import('../src/react/apps/apps-page.tsx');
    await render(AppsPage, { href: base });
    assert.deepEqual([...document.querySelectorAll('.app-name')].map(n => n.textContent), ['Order portal', 'Zoho Books']);
    await click(button('Your systems'));
    assert.deepEqual([...document.querySelectorAll('.app-name')].map(n => n.textContent), ['Order portal'], 'Your systems only');
    const panel = document.querySelector('.app-panel');
    assert.equal(panel.querySelector('a[href*="/open"]'), null, 'no address without access');
    assert.match(panel.textContent, /Chosen people/);
    await click(button('Request access'));
    await change(document.querySelector('[aria-label="Why"]'), 'For deliveries');
    await update(() => { button('Send request').click(); button('Send request').click(); });
    assert.equal(sent.length, 1, 'one request per click burst');
    assert.deepEqual({ ...sent[0], id: undefined }, { id: undefined, app: 'p1', reason: 'For deliveries' });
    assert.match(document.querySelector('.app-panel').textContent, /You asked for access\. Ravi decides\./);
    await click(button('Request an app'));
    assert.equal(document.querySelector('dialog[open] h2').textContent, 'Request an app');
    await click(button('Suggest another app'));
    await change(document.querySelector('[aria-label="App name"]'), 'Delivery tracker');
    await click(button('Send suggestion'));
    assert.equal(sent[1].name, 'Delivery tracker');
    assert.equal([...document.querySelectorAll('button')].some(b => b.textContent === 'Add app'), false);
  }, { url: base, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return session('Member');
    if (path.endsWith('/apps/requests')) { sent.push(JSON.parse(options.body)); requested = !sent[0].name; return response({ id: 'r', state: 'pending' }); }
    if (path.includes('/apps/catalog')) return response({ ...catalogPage, can_manage: false });
    return response({ apps: [{ ...zoho, account: null }], systems: [{ ...portal, requested }], can_manage: false, requests_waiting: 0 });
  } });
});

test('People allowed open a company app through Cuckoo; admins add one and decide requests', async () => {
  const saved = [], decided = [];
  await reactFixture(async ({ render, click, change, button }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { AppsPage } = await import('../src/react/apps/apps-page.tsx');
    await render(AppsPage, { href: base });
    const open = document.querySelector('.app-panel a');
    assert.equal(open.textContent, 'Open Order portal');
    assert.match(open.getAttribute('href'), new RegExp(`/api/identity/workspaces/${w}/apps/systems/p1/open$`));
    assert.equal(open.target, '_blank'); assert.match(open.rel, /noopener/);
    await click(button('1 request'));
    assert.match(document.querySelector('dialog[open]').textContent, /Mina asks to open Order portal/);
    await click(button('Approve'));
    assert.deepEqual(decided, [{ path: 'r1', action: 'approve' }]);
    await click([...document.querySelectorAll('button')].find(b => b.getAttribute('aria-label') === 'Close'));
    await click(button('Add app'));
    await click([...document.querySelectorAll('dialog[open] button')].find(b => b.textContent === "Your company's app"));
    const add = () => [...document.querySelectorAll('dialog[open] button')].find(b => b.textContent === 'Add app');
    assert.equal(add().disabled, true, 'needs a name');
    await change(document.querySelector('[aria-label="Name"]'), 'Delivery tracker');
    await change(document.querySelector('[aria-label="Address"]'), 'https://track.example.test');
    await click(add());
    assert.equal(saved[0].name, 'Delivery tracker'); assert.equal(saved[0].revision, 0); assert.equal(saved[0].audience, 'everyone');
  }, { url: base, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return session('Admin');
    if (path.endsWith('/chat-directory')) return response({ users: [{ id: 'mina', name: 'Mina' }] });
    if (path.endsWith('/apps/requests') && !options.body) return response({ requests: decided.length ? [] : [{ id: 'r1', kind: 'access', app: 'p1', app_name: 'Order portal', name: '', url: '', reason: '', created: 1, requester: 'Mina' }] });
    if (path.includes('/apps/requests/')) { decided.push({ path: path.split('/').at(-1), action: JSON.parse(options.body).action }); return response({ state: 'approved' }); }
    if (path.endsWith('/apps/systems')) { saved.push(JSON.parse(options.body)); return response({ id: 'x' }); }
    if (path.endsWith('/control/connections')) return response({ servers: [], systems: [], connections: [], safety: [], people: [], viewer: 'me', execution_available: true });
    if (path.includes('/apps/catalog')) return response(catalogPage);
    return response({ apps: [], systems: [{ ...portal, can_open: true, url: 'https://orders.example.test', can_decide: true, people: [] }], can_manage: true, requests_waiting: decided.length ? 0 : 1 });
  } });
});

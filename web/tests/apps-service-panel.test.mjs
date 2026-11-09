import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

// The Apps panel is the one place to manage a service (designs/connector-screens 1 and 6).
const w = '11111111-1111-4111-8111-111111111111';
const base = `http://localhost/w/${w}/?view=apps`;
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const session = role => response({ user: { id: 'me', email: 'me@test' }, workspaces: [{ id: w, name: 'Mehta', role }] });
const now = Math.floor(Date.now() / 1000);
const gmail = { id: 'c-gmail', server: 'gmail', name: 'Gmail', kind: 'service', provider: 'composio', account: 'orders@mehta.test', status: 'connected', rules: 1,
  state: 'writes_paused', paused_action: 'act-1', last_used: { at: now - 600, agent_name: 'Atlas' }, connected: { at: now - 60, by_name: 'Riya Kapoor' }, via: 'Composio',
  actions: [{ id: 'gmail.find_emails', title: 'Find emails', meaning: 'read', mode: 'allow' }, { id: 'gmail.send_email', title: 'Send an email', meaning: 'sends_outside', mode: 'ask' }],
  agents: [{ id: 'a1', name: 'Atlas', avatar: 'robot-01' }] };
const operation = (id, title, effect, mode, meaning, extra = {}) => ({ id, title, effect, mode, needs_other: false, revision: 2, default: true, meaning, meaning_reviewed: true, meaning_revision: 0, ...extra });
const control = (extra = {}) => ({ servers: [], systems: [{ server: 'gmail', approved: true, revision: 3 }],
  connections: [{ id: 'c-gmail', server: 'gmail', enabled: true, revision: 5, definition: { id: 'gmail', name: 'Gmail', operations: [] } }],
  safety: [{ connection: 'c-gmail', agents: ['Atlas'], operations: [operation('gmail.find_emails', 'Find emails', 'read', 'allow', 'read'),
    operation('gmail.send_email', 'Send an email', 'write', 'ask', 'sends_outside', { meaning_reviewed: false })] }],
  people: [], viewer: 'me', execution_available: true, ...extra });
const action = { id: 'act-1', connection: 'c-gmail', operation: 'gmail.send_email', title: 'Send an email', effect: 'write', arguments: { to: 'priya@sharma.test' },
  state: 'uncertain', result: null, chat: 'orders', thread: null, job: 'j', approver: 'me', created: now - 3600, updated: now, agent: 'a1', agent_name: 'Atlas',
  account_name: 'Gmail', approver_name: 'Riya', can_decide: false, requester_name: 'Arjun Mehta', conversation_name: '#orders', settle: { can_settle: true, checks: [] } };
async function apps(render) {
  const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
  const { AppsPage } = await import('../src/react/apps/apps-page.tsx');
  await render(AppsPage, { href: base });
  return document.querySelector('.app-panel');
}

test('Admins change a rule and a kind in the panel; a double click saves once and a refused save says why', async () => {
  const writes = []; let release; let fail = true; let reads = 0;
  await reactFixture(async ({ render, update, click, button }) => {
    const panel = await apps(render);
    assert.match(panel.textContent, /What agents may doYou can change these/);
    const block = panel.querySelector('[aria-label="Send an email workspace rule"] input[value=block]');
    const before = reads;
    await update(() => { block.click(); block.click(); });
    assert.equal(writes.length, 1, 'one save per click burst');
    await update(() => release());
    assert.deepEqual(writes[0], { path: '/connector-rules', body: { connection: 'c-gmail', operation: 'gmail.send_email', mode: 'block', needs_other: false, revision: 2 } });
    assert.match(panel.textContent, /Settings changed; reload before saving/);
    assert.ok(reads > before, 'a refused save reloads the rules');
    fail = false;
    // Reading actions have no kinds to choose and no second approver to require.
    assert.equal(panel.querySelector('[aria-label^="What Find emails does"]'), null);
    const kind = button('What Send an email does: Sends outside');
    assert.match(kind.parentElement.textContent, /Sends outside ▾ · suggested/);
    await click(kind);
    assert.equal(kind.getAttribute('aria-expanded'), 'true');
    assert.deepEqual([...panel.querySelectorAll('[role=menuitemradio]')].map(item => item.textContent), ['Changes', 'Money', 'Sends outside', 'Deletes']);
    assert.equal(document.activeElement, panel.querySelector('[role=menuitemradio]'), 'the menu takes focus');
    await update(() => document.activeElement.dispatchEvent(new window.KeyboardEvent('keydown', { key: 'Escape', bubbles: true })));
    assert.equal(panel.querySelector('[role=menu]'), null); assert.equal(document.activeElement, kind);
    await click(kind);
    await click([...panel.querySelectorAll('[role=menuitemradio]')].find(item => item.textContent === 'Money'));
    assert.deepEqual(writes.at(-1), { path: '/action-meanings', body: { connection: 'c-gmail', operation: 'gmail.send_email', meaning: 'money', revision: 0 } });
    assert.match(panel.textContent, /Send an email: Money\./);
    await click(button('What Send an email does: Sends outside'));
    await click(panel.querySelector('[role=menuitemcheckbox]'));
    assert.deepEqual(writes.at(-1).body, { connection: 'c-gmail', operation: 'gmail.send_email', mode: 'ask', needs_other: true, revision: 2 });
  }, { url: base, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return session('Admin');
    if (options.body) {
      writes.push({ path: path.slice(path.lastIndexOf('/')), body: JSON.parse(options.body) });
      if (fail) { await new Promise(resolve => { release = resolve; }); return response({ error: 'Settings changed; reload before saving' }, 400); }
      return response({ saved: true, revision: 1 });
    }
    if (path.endsWith('/control/connections')) { reads++; return response(control()); }
    return response({ apps: [{ ...gmail, state: 'ok' }], systems: [], can_manage: true, requests_waiting: 0 });
  } });
});

test('Disconnect asks in the page, sends one request and says when the app did not confirm', async () => {
  const writes = []; let asked = 0; let gone = false;
  await reactFixture(async ({ render, update, click, button }) => {
    globalThis.confirm = () => { asked++; return true; };
    await apps(render);
    await click(button('Disconnect Gmail…'));
    assert.equal(document.querySelector('dialog[open] h2').textContent, 'Disconnect Gmail?');
    assert.match(document.querySelector('dialog[open]').textContent, /including Atlas.*safe defaults/);
    await click(button('Cancel'));
    assert.equal(writes.length, 0); assert.equal(document.querySelector('dialog[open]'), null);
    await click(button('Disconnect Gmail…'));
    await update(() => { button('Disconnect Gmail').click(); button('Disconnect Gmail').click(); });
    assert.deepEqual(writes, [{ path: '/systems', body: { server: 'gmail', approved: false, revision: 3 } }]);
    assert.equal(asked, 0, 'no browser confirm');
    assert.match(document.querySelector('.apps-status').textContent, /Gmail is disconnected in Cuckoo\. Gmail didn’t confirm removing Cuckoo’s access/);
    assert.equal(document.querySelector('.app-panel'), null);
  }, { url: base, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return session('Admin');
    if (options.body) { writes.push({ path: path.slice(path.lastIndexOf('/')), body: JSON.parse(options.body) }); gone = true; return response({ saved: true, provider_revoked: false }); }
    if (path.endsWith('/control/connections')) return response(control());
    return response({ apps: gone ? [] : [gmail], systems: [], can_manage: true, requests_waiting: 0 });
  } });
});

test('Reconnect signs the same connection in again on the app’s own page', async () => {
  const writes = [];
  await reactFixture(async ({ render, click, button, navigation }) => {
    const panel = await apps(render);
    assert.match(panel.textContent, /The sign-in expired\. Agents can’t use Gmail until you reconnect it/);
    assert.match(panel.textContent, /Sign-inExpired · through Composio/);
    await click(button('Reconnect'));
    assert.deepEqual(writes, [{ id: 'c-gmail', server: 'gmail', enabled: true, revision: 5 }]);
    assert.deepEqual(navigation, ['https://connect.example.test/sign-in/again']);
  }, { url: base, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return session('Admin');
    if (options.body) { writes.push(JSON.parse(options.body)); return response({ redirect_url: 'https://connect.example.test/sign-in/again' }); }
    if (path.endsWith('/control/connections')) return response(control());
    return response({ apps: [{ ...gmail, state: 'reconnect' }], systems: [], can_manage: true, requests_waiting: 0 });
  } });
});

const states = [gmail,
  { ...gmail, id: 'c-zoho', server: 'zoho', name: 'Zoho Books', state: 'reconnect', paused_action: null, last_used: { at: now - 86400, agent_name: 'Ledger' } },
  { ...gmail, id: 'c-slack', server: 'slack', name: 'Slack', state: 'paused', paused_action: null },
  { ...gmail, id: 'c-hubspot', server: 'hubspot', name: 'HubSpot', state: 'ok', paused_action: null, last_used: null, agents: [] },
  { ...gmail, id: 'c-cal', server: 'calendar', name: 'Google Calendar', state: 'ok', paused_action: null, last_used: { at: now - 3600, agent_name: 'Atlas' } }];

test('Cards and the panel name each state and when the app was used; Check what happened opens the outcome', async () => {
  const reads = [];
  await reactFixture(async ({ render, click, button }) => {
    const panel = await apps(render);
    assert.deepEqual([...document.querySelectorAll('.app-card')].map(card => card.querySelector('.app-status')?.textContent ?? ''),
      ['Writes paused', 'Reconnect', 'Paused by Cuckoo', 'Not used yet', '']);
    assert.deepEqual([...document.querySelectorAll('.app-foot > span:last-child')].map(foot => foot.textContent),
      ['Used 10 min ago', 'Used yesterday', 'Used 10 min ago', 'Connected today', 'Used 1 hour ago']);
    assert.match(document.querySelector('.apps-needs').textContent, /2 need you/);
    assert.match(panel.textContent, /Writes are paused\..*Reading still works/);
    assert.match(panel.textContent, /Connected byRiya Kapoor, today/);
    assert.match(panel.textContent, /Last used10 min ago, by Atlas/);
    assert.match(panel.textContent, /Sign-inWorks · through Composio/);
    await click(button('Check what happened'));
    assert.ok(reads.some(path => path.endsWith('/control/actions/act-1')));
    assert.equal(document.querySelector('dialog[open] h2').textContent, 'Did this go out?');
    await click(document.querySelectorAll('.app-card')[2]);
    assert.match(document.querySelector('.app-panel').textContent, /Cuckoo paused Slack for a short while/);
  }, { url: base, fetch: async path => {
    reads.push(path);
    if (path.endsWith('/session')) return session('Admin');
    if (path.endsWith('/control/connections')) return response(control());
    if (path.includes('/control/actions/')) return response(action);
    return response({ apps: states, systems: [], can_manage: true, requests_waiting: 0 });
  } });
});

test('Members read the same panel; without the new fields it falls back to today’s status', async () => {
  const reads = [];
  await reactFixture(async ({ render }) => {
    const panel = await apps(render);
    assert.match(panel.textContent, /Service · Needs reconnecting/);
    assert.match(panel.textContent, /until an admin reconnects it/);
    assert.match(panel.textContent, /Sign-inExpired$/, 'no connection service and no last use without the fields');
    assert.doesNotMatch(panel.textContent, /Last used|Connected by|You can change these/);
    assert.equal(document.querySelector('.app-card .app-status').textContent, 'Reconnect');
    assert.equal(reads.some(path => path.endsWith('/control/connections')), false, 'members never ask for admin data');
  }, { url: base, fetch: async path => {
    reads.push(path);
    if (path.endsWith('/session')) return session('Member');
    const { state, paused_action, last_used, connected, via, ...today } = gmail;
    return response({ apps: [{ ...today, account: null, status: 'reconnect' }], systems: [], can_manage: false, requests_waiting: 0 });
  } });
});

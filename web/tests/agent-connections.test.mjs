import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

// Connections (schema 132): an agent's owner adds an app for that agent alone,
// changes its one rule set, signs in again and disconnects it, all on the agent's page.
const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=agents&selected=nova&section=connections`;
const agent = { id: 'nova', name: 'Nova', role: 'Orders', model: 'm', audience: 'everyone', revision: 1, settings: {}, editable: true };
const directory = { agents: [agent], role: 'Member', default_model: 'm', system_prompt: '', providers: [] };
const operation = (id, title, effect, mode, meaning) => ({ id, title, effect, meaning, workspace_mode: mode, needs_other: false, agent_mode: null, mode, revision: 0, rule_revision: 3 });
const gmail = (extra = {}) => ({ id: 'c1', server: 'gmail', name: 'Gmail', account: 'orders@example.test', revision: 4, on: true, own: true, provider: 'composio', state: 'ok', operations: [
  operation('gmail.find', 'Find emails', 'read', 'allow', 'read'), operation('gmail.send', 'Send an email', 'write', 'ask', 'sends_outside')], ...extra });
const controls = (connections, extra = {}) => ({ can_manage: true, can_manage_connections: false, viewer: 'ravi', connections, actions: [], execution_available: true, ...extra });
const catalog = { categories: [{ id: 'communication', name: 'Communication', count: 1 }], next_cursor: null, can_manage: true,
  apps: [{ id: 'gmail', name: 'Gmail', description: 'Read and send email', category: 'communication', kinds: ['read', 'sends_outside'], actions: 2, state: 'available', connection: null }] };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
function server(state, writes, reads = []) {
  return async (path, options = {}) => {
    const short = path.replace(`/api/identity/workspaces/${w}`, '');
    if (options.body) { writes.push({ path: short, body: JSON.parse(options.body) }); return response(state.answer ?? { saved: true }); }
    reads.push(short);
    if (short.startsWith('/apps/catalog')) return response(catalog);
    if (short.endsWith('/control')) return response(controls(state.connections, state.extra));
    return response({ systems: [], can_connect: true });
  };
}
async function show(render) {
  const { ConnectionsTab } = await import('../src/react/agents/connections-tab.tsx');
  await render(ConnectionsTab, { agent, directory, saved: async () => {} });
}
const button = label => [...document.querySelectorAll('button')].find(node => node.textContent.trim() === label);

test('the owner adds an app for this agent: the catalog is the agent’s and one request starts the sign-in', async () => {
  const writes = [], reads = [], state = { connections: [], answer: { redirect_url: 'https://accounts.example.test/sign-in' } };
  await reactFixture(async ({ render, click, update, navigation }) => {
    await show(render);
    assert.match(document.body.textContent, /No apps yet\. Add one for Nova/);
    assert.match(document.body.textContent, /belongs to Nova alone/);
    assert.ok(document.querySelector('[aria-label="MCP servers"]'), 'MCP servers have their own group');
    await click(button('＋ Add app'));
    assert.ok(reads.some(path => path.startsWith('/apps/catalog') && path.includes('agent=nova')), 'the catalog is read for this agent');
    await click([...document.querySelectorAll('.catalog-tile button, button.catalog-tile')].find(node => /Gmail/.test(node.textContent)) ?? button('Connect'));
    const dialog = [...document.querySelectorAll('dialog')].find(node => /Connect Gmail/.test(node.textContent));
    assert.match(dialog.textContent, /What Nova could do/);
    assert.match(dialog.textContent, /Only Nova can use this Gmail account/);
    const go = [...dialog.querySelectorAll('button')].find(node => /Continue to Gmail/.test(node.textContent));
    await update(() => { go.click(); go.click(); });
    assert.equal(writes.length, 1, 'one request despite a double click');
    assert.equal(writes[0].path, '/apps/connect');
    assert.deepEqual({ ...writes[0].body, request: undefined }, { app: 'gmail', request: undefined, agent: 'nova' });
    assert.deepEqual(navigation, ['https://accounts.example.test/sign-in'], 'the app’s own sign-in page opens');
  }, { url, fetch: server(state, writes, reads) });
});

test('an own app has one rule set its owner changes, with no admin limit and no switch', async () => {
  const writes = [], state = { connections: [gmail()] };
  await reactFixture(async ({ render, update, click }) => {
    await show(render);
    const app = document.querySelector('[aria-label="Gmail"]');
    assert.equal(app.textContent.includes('admin limit'), false);
    assert.equal(app.querySelector('[role=switch]'), null, 'it is this agent’s alone: nothing to switch');
    const allow = app.querySelector('[aria-label="Send an email rule"] input[value=allow]');
    assert.equal(allow.disabled, false, 'the owner may loosen its own rule');
    await update(() => allow.click());
    assert.equal(writes[0].path, '/control/connector-rules');
    assert.deepEqual(writes[0].body, { connection: 'c1', operation: 'gmail.send', mode: 'allow', needs_other: false, revision: 3 });
    await click([...app.querySelectorAll('button')].find(node => node.textContent === 'Disconnect'));
    assert.equal(writes[1].path, '/control/connections');
    assert.deepEqual(writes[1].body, { id: 'c1', server: 'gmail', enabled: false, revision: 4, agent: 'nova' });
  }, { url, fetch: server(state, writes) });
});

test('an expired sign-in is renewed from the agent’s page; people who can’t change the agent get no controls', async () => {
  const writes = [], state = { connections: [gmail({ state: 'reconnect' })], answer: { redirect_url: 'https://accounts.example.test/again' } };
  await reactFixture(async ({ render, click, navigation }) => {
    await show(render);
    const app = document.querySelector('[aria-label="Gmail"]');
    assert.match(app.textContent, /The sign-in expired\. Nova can’t use Gmail until someone signs in again/);
    await click([...app.querySelectorAll('button')].find(node => node.textContent === 'Sign in again'));
    assert.deepEqual({ ...writes[0].body, request: undefined }, { app: 'gmail', request: undefined, agent: 'nova' });
    assert.deepEqual(navigation, ['https://accounts.example.test/again']);
  }, { url, fetch: server(state, writes) });
  await reactFixture(async ({ render }) => {
    await show(render);
    assert.equal(button('＋ Add app'), undefined);
    assert.equal(button('Disconnect'), undefined);
    assert.equal(document.querySelector('[aria-label="Send an email rule"] input[value=allow]').disabled, true);
  }, { url, fetch: server({ connections: [gmail()], extra: { can_manage: false } }, []) });
});

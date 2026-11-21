import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

// Settings › Apps (schema 132): admins see every connection under its app,
// whose it is, and remove one with a single request.
const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=settings&section=connections`;
const app = (id, name, owner, extra = {}) => ({ id, server: name.toLowerCase(), name, kind: 'service', provider: 'composio', account: `${id}@example.test`, status: 'connected', state: 'ok',
  actions: [], agents: [], rules: 0, revision: 7, owner, connected: { at: 1763283720, by_name: 'Ravi Kumar' }, last_used: null, ...extra });
const response = data => ({ ok: true, json: async () => data });

const allowedApp = (id, name) => ({ id, name, description: '', category: 'files', allowance: 'allowed', note: '', revision: 1, decided: null, connections: 1, waiting: 0 });
const answer = state => async (path, options = {}) => {
  if (options.body) { state.writes.push({ path, body: JSON.parse(options.body) }); return response({ saved: true }); }
  if (path.endsWith('/apps/allowed')) return response({ apps: [allowedApp('gmail', 'Gmail')], systems: [] });
  if (path.endsWith('/apps/requests')) return response({ requests: [] });
  return response({ apps: state.apps, systems: [], can_manage: state.manage, requests_waiting: 0 });
};

test('connections sit under their app, show whose they are, and Remove sends one request', async () => {
  const state = { writes: [], manage: true, apps: [app('c1', 'Gmail', { id: 'nova', name: 'Nova' }), app('c2', 'Payroll', { id: 'hr', name: 'HR Desk' }, { provider: 'system', state: 'reconnect' })] };
  await reactFixture(async ({ render, update, click, button }) => {
    const { ConnectionsSettings } = await import('../src/react/settings/apps-settings.tsx');
    await render(ConnectionsSettings);
    assert.equal(document.querySelector('.settings-apps h2').textContent, 'Apps');
    const gmail = document.querySelector('.settings-app');
    assert.match(gmail.querySelector('.settings-row').textContent, /Gmail1 connection/);
    assert.equal(gmail.querySelector('.settings-connections'), null, 'a healthy app starts closed');
    await click(button('Connections of Gmail'));
    const line = gmail.querySelector('.settings-connections li');
    assert.match(line.textContent, /c1@example\.test.*by Ravi Kumar.*not used yet/);
    assert.match(line.querySelector('a').getAttribute('href'), /view=agents&selected=nova&section=connections$/);
    // An MCP server belongs to no app in the list; it keeps its own section and is removed on its agent's page.
    const other = document.querySelector('[aria-label="Other connections"] li');
    assert.match(other.textContent, /Payroll.*MCP server.*HR Desk/);
    assert.match(other.querySelector('.settings-connection-problem').textContent, /Needs a new sign-in/);
    assert.equal(other.querySelector('button'), null);
    const remove = line.querySelector('button');
    assert.equal(remove.getAttribute('aria-label'), 'Remove Gmail from Nova');
    state.apps = [state.apps[1]];
    await update(() => { remove.click(); remove.click(); });
    assert.equal(state.writes.length, 1, 'one request despite a double click');
    assert.ok(state.writes[0].path.endsWith('/control/connections'));
    assert.deepEqual(state.writes[0].body, { id: 'c1', server: 'gmail', enabled: false, revision: 7, agent: 'nova' });
    assert.equal(gmail.querySelector('.settings-connections'), null, 'the list is read again');
    assert.match(document.body.textContent, /Gmail removed\./);
  }, { url, fetch: answer(state) });
});

test('a connection that needs attention opens its app, and people who cannot manage connections get no Remove', async () => {
  const state = { writes: [], manage: false, apps: [app('c1', 'Gmail', { id: 'nova', name: 'Nova' }, { state: 'reconnect' })] };
  await reactFixture(async ({ render }) => {
    const { ConnectionsSettings } = await import('../src/react/settings/apps-settings.tsx');
    await render(ConnectionsSettings);
    const lines = document.querySelectorAll('.settings-connections li');
    assert.equal(lines.length, 1, 'open without a click, so the problem is seen');
    assert.match(lines[0].textContent, /Needs a new sign-in/);
    assert.equal(lines[0].querySelector('button'), null);
  }, { url, fetch: answer(state) });
});

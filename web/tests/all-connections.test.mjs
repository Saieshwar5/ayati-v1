import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

// Settings › Connections (schema 132): admins see every app connected in the
// workspace, whose it is, and remove one with a single request.
const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=settings&section=connections`;
const app = (id, name, owner, extra = {}) => ({ id, server: name.toLowerCase(), name, kind: 'service', provider: 'composio', account: `${id}@example.test`, status: 'connected', state: 'ok',
  actions: [], agents: [], rules: 0, revision: 7, owner, connected: { at: 1763283720, by_name: 'Ravi Kumar' }, last_used: null, ...extra });
const response = data => ({ ok: true, json: async () => data });

test('every connection shows whose it is, and Remove sends one request for that agent’s connection', async () => {
  const writes = [];
  let apps = [app('c1', 'Gmail', { id: 'nova', name: 'Nova' }), app('c2', 'Payroll', { id: 'hr', name: 'HR Desk' }, { provider: 'system', state: 'reconnect' })];
  await reactFixture(async ({ render, update }) => {
    const { AllConnections } = await import('../src/react/settings/all-connections.tsx');
    await render(AllConnections);
    const rows = [...document.querySelectorAll('.settings-row')];
    assert.equal(rows.length, 2);
    assert.match(rows[0].textContent, /Gmail.*c1@example\.test.*by Ravi Kumar.*not used yet/);
    assert.match(rows[0].querySelector('a').getAttribute('href'), /view=agents&selected=nova&section=connections$/);
    assert.match(rows[1].textContent, /Payroll.*MCP server.*Needs a new sign-in.*HR Desk/);
    assert.equal(rows[1].querySelector('button'), null, 'an MCP server is removed on its agent’s page');
    const remove = rows[0].querySelector('button');
    assert.equal(remove.getAttribute('aria-label'), 'Remove Gmail from Nova');
    apps = [apps[1]];
    await update(() => { remove.click(); remove.click(); });
    assert.equal(writes.length, 1, 'one request despite a double click');
    assert.ok(writes[0].path.endsWith('/control/connections'));
    assert.deepEqual(writes[0].body, { id: 'c1', server: 'gmail', enabled: false, revision: 7, agent: 'nova' });
    assert.equal(document.querySelectorAll('.settings-row').length, 1, 'the list is read again');
    assert.match(document.body.textContent, /Gmail removed\./);
  }, { url, fetch: async (path, options = {}) => {
    if (options.body) { writes.push({ path, body: JSON.parse(options.body) }); return response({ saved: true }); }
    return response({ apps, systems: [], can_manage: true, requests_waiting: 0 });
  } });
});

test('people who cannot manage connections get no Remove', async () => {
  await reactFixture(async ({ render }) => {
    const { AllConnections } = await import('../src/react/settings/all-connections.tsx');
    await render(AllConnections);
    assert.equal(document.querySelectorAll('.settings-row').length, 1);
    assert.equal(document.querySelector('.settings-row button'), null);
  }, { url, fetch: async () => response({ apps: [app('c1', 'Gmail', { id: 'nova', name: 'Nova' })], systems: [], can_manage: false, requests_waiting: 0 }) });
});

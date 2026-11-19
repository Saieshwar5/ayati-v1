import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

// Agent → Connections → Your own computer (B3): machines the workspace runs itself
// are added and chosen on the agent's page. There is no Settings page for computers.
const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=agents&selected=atlas&section=connections`;
const response = data => ({ ok: true, json: async () => data });
const field = label => [...document.querySelectorAll('label')].find(node => node.textContent.startsWith(label)).querySelector('input');
const radio = label => [...document.querySelectorAll('.agent-own-computers label')].find(node => node.textContent.startsWith(label)).querySelector('input');
const agent = { id: 'atlas', name: 'Atlas', role: 'Orders', model: 'm', audience: 'everyone', revision: 3, settings: {}, editable: true };
const directory = { agents: [agent], role: 'Admin', default_model: 'm', system_prompt: '', providers: [] };
const office = { id: 'c1', name: 'Office PC', state: 'online', platform: 'Windows 11', revision: 3, busy: false, agents: [{ id: 'atlas', name: 'Atlas' }] };
const short = path => path.replace(`/api/identity/workspaces/${w}`, '');
/** The Connections tab's other reads answer empty; only computers matter here. */
const other = path => short(path).endsWith('/control') ? { can_manage: true, can_manage_connections: false, viewer: 'ravi', connections: [], actions: [], execution_available: true }
  : { systems: [], can_connect: true };
async function show(render, extra = {}) {
  const { ConnectionsTab } = await import('../src/react/agents/connections-tab.tsx');
  await render(ConnectionsTab, { agent, directory: { ...directory, ...extra }, saved: async () => {} });
}

test('an admin adds a computer once on the agent’s page, sees its one-time code and chooses it for the agent', async () => {
  const writes = [];
  let computers = [];
  let resolveAdd;
  await reactFixture(async ({ render, button, click, change, update }) => {
    await show(render);
    assert.match(document.body.textContent, /No computers added yet/);
    assert.match(document.body.textContent, /permissions of the account that runs the connector/);
    await change(field('Computer name'), '  Office PC ');
    // Two quick clicks send one request.
    const adding = button('Add computer');
    await update(() => { adding.click(); adding.click(); });
    assert.equal(writes.filter(entry => entry.path.endsWith('/workspace-computers')).length, 1);
    computers = [{ ...office, agents: [] }];
    await update(resolveAdd);
    const dialog = [...document.querySelectorAll('dialog')].find(node => /Connect Office PC/.test(node.textContent));
    assert.match(dialog.textContent, /--computer c1 --code ABCD-EFGH-JKLM-NPQR/);
    assert.match(dialog.textContent, /works once, for 15 minutes/);
    await click(button('Done'));
    assert.match(document.body.textContent, /Office PCOnline · Windows 11No agents use it yet/);
    assert.equal(radio('No computer of yours').checked, true, 'nothing is chosen until an admin chooses');

    computers = [office];
    await click(radio('Office PC'));
    const saved = writes.find(entry => entry.path.endsWith('/agent-profiles/atlas/computer/own'));
    assert.deepEqual(saved.body, { computer: 'c1' });
    assert.equal(radio('Office PC').checked, true);
    assert.match(document.body.textContent, /Used by Atlas/);
    assert.match(document.body.textContent, /Atlas works on Office PC from its next task/);

    computers = [{ ...office, agents: [] }];
    await click(radio('No computer of yours'));
    assert.deepEqual(writes.at(-1).body, { computer: null });
  }, { url, fetch: async (path, options = {}) => {
    const body = options.body ? JSON.parse(options.body) : undefined;
    if (body) writes.push({ path: short(path), body });
    if (body && path.endsWith('/workspace-computers')) {
      assert.equal(body.name, 'Office PC');
      assert.match(body.id, /^[0-9a-f-]{36}$/);
      return new Promise(resolve => { resolveAdd = () => resolve(response({ id: 'c1', code: 'ABCD-EFGH-JKLM-NPQR', expires_minutes: 15 })); });
    }
    if (body) return response({});
    if (path.endsWith('/workspace-computers')) return response({ computers, can_change: true });
    return response(other(path));
  } });
});

test('a failed choice leaves the agent where it was, and a failed removal keeps the computer listed', async () => {
  await reactFixture(async ({ render, button, click }) => {
    await show(render);
    await click(radio('No computer of yours'));
    assert.match(document.body.textContent, /This computer changed; reload/);
    assert.equal(radio('Office PC').checked, true, 'Atlas still works on the office PC');
    await click(button('Remove'));
    const dialog = [...document.querySelectorAll('dialog')].find(node => /Remove Office PC/.test(node.textContent));
    assert.match(dialog.textContent, /signed out/);
    await click(button('Remove computer'));
    assert.match(dialog.textContent, /This computer changed; reload/);
    assert.match(document.body.textContent, /Office PC/);
  }, { url, fetch: async (path, options = {}) => {
    if (options.body) return { ok: false, status: 409, json: async () => ({ error: 'This computer changed; reload before trying again' }) };
    if (path.endsWith('/workspace-computers')) return response({ computers: [office], can_change: true });
    return response(other(path));
  } });
});

test('members see where the agent works without changing it, and nothing when no computer was added', async () => {
  let computers = [{ ...office, state: 'offline' }];
  const fetch = async path => response(path.endsWith('/workspace-computers') ? { computers, can_change: false } : other(path));
  await reactFixture(async ({ render }) => {
    await show(render);
    assert.match(document.body.textContent, /Office PCOffline · Windows 11Used by Atlas/);
    assert.equal(radio('Office PC').disabled, true);
    assert.equal([...document.querySelectorAll('button')].some(node => ['Remove', 'Add computer', 'New code'].includes(node.textContent)), false);
    assert.match(document.body.textContent, /Only admins add computers and choose where Atlas works/);
  }, { url, fetch });
  computers = [];
  await reactFixture(async ({ render }) => {
    await show(render);
    assert.equal(document.querySelector('[aria-label="Computers"]'), null, 'no Computers group for a member with nothing to see');
  }, { url, fetch });
});

test('Cuckoo’s computer card says when the agent works on the workspace’s own machine instead', async () => {
  const offer = { sleep_after_minutes: 5, sizes: [{ size: 'standard', cpu: 2, memory_gb: 4, disk_gb: 20, usd_per_hour: 0.17 }] };
  await reactFixture(async ({ render }) => {
    await show(render, { computers: offer });
    assert.match(document.body.textContent, /Atlas works on Office PC, your own computer \(below\), instead of this one/);
    assert.equal(radio('Cuckoo’s computer').checked, false);
  }, { url, fetch: async path => {
    if (path.endsWith('/workspace-computers')) return response({ computers: [office], can_change: true });
    if (path.endsWith('/computer')) return response({ configured: true, settings: {}, computer: null, month: { seconds: 0, charge_usd: 0 }, sleep_after_minutes: 5 });
    return response(other(path));
  } });
});

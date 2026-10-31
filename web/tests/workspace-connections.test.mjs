import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
const workspace = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${workspace}/?view=settings&scope=workspace&section=connections`;
const operation = { id: 'order.update', title: 'Update order', effect: 'write', schema: {} };
const server = { id: 'orders', name: 'Orders', account: 'Sample business', operations: [operation] };
const person = { id: 'member', name: 'Priya', role: 'Member' };
const connection = { id: 'connection', server: 'orders', enabled: true, revision: 4, definition: server };
const safety = { connection: 'connection', agents: ['Atlas'], operations: [{ id: 'order.update', title: 'Update order', effect: 'write', mode: 'ask', needs_other: false, revision: 0, default: true }] };
const state = extra => ({ servers: [server], systems: [], connections: [], safety: [safety], people: [person], viewer: 'owner', execution_available: true, ...extra });
const response = data => ({ ok: true, status: 200, json: async () => data });

test('Owner approval is explicit and does not connect an account or assign agent authority', async () => {
  const writes = []; let approved = false;
  await reactFixture(async ({ render, button, click }) => {
    const { WorkspaceConnections } = await import('../src/react/settings/workspace-connections.tsx');
    await render(WorkspaceConnections);
    assert.equal(writes.length, 0);
    assert.equal(button('Connect account'), undefined);
    assert.match(document.querySelector('article').textContent, /Sample business/);
    await click(button('Approve system'));
    assert.equal(button('Connect account').disabled, false);
    assert.deepEqual(writes.map(w => w.body), [{ server: 'orders', approved: true, revision: 0 }]);
    assert.ok(writes[0].path.endsWith('/control/systems'));
  }, { url, fetch: async (path, options = {}) => {
    if (options.body) { writes.push({ path, body: JSON.parse(options.body) }); approved = true; return response({ saved: true }); }
    return response(state({ systems: approved ? [{ server: 'orders', approved: true, revision: 1 }] : [] }));
  } });
});

test('connecting guards double submission and reuses the exact identity after an uncertain response', async () => {
  const writes = []; let release; let fail = true;
  await reactFixture(async ({ render, button, click, update }) => {
    const { WorkspaceConnections } = await import('../src/react/settings/workspace-connections.tsx');
    await render(WorkspaceConnections);
    await update(() => { button('Connect account').click(); button('Connect account').click(); });
    assert.equal(writes.length, 1);
    await update(() => release());
    assert.match(document.body.textContent, /Response unavailable/);
    fail = false;
    await click(button('Connect account')); await update(() => release());
    assert.deepEqual(writes[0], writes[1]);
    assert.match(writes[0].id, /^[a-f0-9-]{36}$/);
    assert.equal(writes[0].revision, 0);
  }, { url, fetch: async (_, options = {}) => {
    if (options.body) { writes.push(JSON.parse(options.body)); await new Promise(resolve => { release = resolve; }); return fail ? { ok: false, status: 503, json: async () => ({ error: 'Response unavailable' }) } : response({ saved: true }); }
    return response(state({ systems: [{ server: 'orders', approved: true, revision: 1 }] }));
  } });
});

test('safety rules save once per action, keep a failed choice and ask who else must approve', async () => {
  const writes = []; let release; let fail = true;
  await reactFixture(async ({ render, click, update }) => {
    const { WorkspaceConnections } = await import('../src/react/settings/workspace-connections.tsx');
    await render(WorkspaceConnections);
    const panel = document.querySelector('[aria-label="Safety for Orders"]');
    assert.match(panel.textContent, /used by Atlas/);
    assert.match(panel.textContent, /New connectors start safe/);
    const block = panel.querySelector('input[value=block]');
    await update(() => { block.click(); block.click(); });
    assert.equal(writes.length, 1); await update(() => release());
    assert.match(panel.textContent, /Settings changed; reload/);
    assert.deepEqual(writes[0], { connection: 'connection', operation: 'order.update', mode: 'block', needs_other: false, revision: 0 });
    fail = false;
    await click(panel.querySelector('input[type=checkbox]'));
    assert.deepEqual(writes[1], { connection: 'connection', operation: 'order.update', mode: 'ask', needs_other: true, revision: 0 });
  }, { url, fetch: async (_, options = {}) => {
    if (options.body) {
      writes.push(JSON.parse(options.body));
      if (fail) { await new Promise(resolve => { release = resolve; }); return { ok: false, status: 400, json: async () => ({ error: 'Settings changed; reload before saving' }) }; }
      return response({ saved: true });
    }
    return response(state({ connections: [connection] }));
  } });
});

test('everyone may create agents, so there is no creator permission to grant', async () => {
  await reactFixture(async ({ render }) => {
    const { WorkspaceConnections } = await import('../src/react/settings/workspace-connections.tsx');
    await render(WorkspaceConnections);
    assert.doesNotMatch(document.body.textContent, /Agent creators/);
    assert.equal(document.querySelector('input[aria-label*="create workspace agents"]'), null);
  }, { url, fetch: async () => response(state()) });
});

test('revoked workspace access clears account controls and a late response cannot repopulate a remount', async () => {
  let deny = false; let release;
  await reactFixture(async ({ render, button, click, update }) => {
    const { WorkspaceConnections } = await import('../src/react/settings/workspace-connections.tsx');
    await render(WorkspaceConnections, { key: 'owner' });
    assert.ok(document.querySelector('[aria-label="Safety for Orders"]')); deny = true;
    await click(button('Refresh connections'));
    assert.equal(document.querySelector('[aria-label="Safety for Orders"]'), null);
    assert.match(document.querySelector('[role=alert]').textContent, /Owner access required/);
    deny = 'late'; await render(WorkspaceConnections, { key: 'old-identity' });
    await render(() => null); await update(() => release());
    assert.equal(document.querySelector('article'), null);
  }, { url, fetch: async () => {
    if (deny === 'late') { await new Promise(resolve => { release = resolve; }); return response(state({ connections: [connection] })); }
    return deny ? { ok: false, status: 400, json: async () => ({ error: 'Owner access required' }) } : response(state({ connections: [connection] }));
  } });
});

test('missing operator configuration can still be revoked and unavailable execution can disconnect', async () => {
  await reactFixture(async ({ render, button }) => {
    const { WorkspaceConnections } = await import('../src/react/settings/workspace-connections.tsx');
    await render(WorkspaceConnections);
    assert.equal(button('Disconnect').disabled, false);
    assert.equal(button('Remove approval').disabled, false);
    assert.match(document.body.textContent, /Configuration unavailable/);
  }, { url, fetch: async () => response(state({ servers: [], connections: [connection], systems: [{ server: 'orders', approved: true, revision: 1 }], execution_available: false })) });
});

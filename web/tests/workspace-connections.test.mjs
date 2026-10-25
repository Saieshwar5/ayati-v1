import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
const workspace = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${workspace}/?view=settings&scope=workspace&section=connections`;
const operation = { id: 'order.update', title: 'Update order', effect: 'write', schema: {} };
const server = { id: 'orders', name: 'Orders', account: 'Sample business', operations: [operation] };
const person = { id: 'member', name: 'Priya', role: 'User', can_create: false, revision: 0 };
const connection = { id: 'connection', server: 'orders', enabled: true, revision: 4, definition: server };
const state = extra => ({ servers: [server], systems: [], connections: [], people: [person], delegations: [], viewer: 'owner', execution_available: true, ...extra });
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

test('delegation retains a failed draft, bounds its revision and names the responsible Owner', async () => {
  const writes = []; let release;
  await reactFixture(async ({ render, button, click, change, update }) => {
    const { WorkspaceConnections } = await import('../src/react/settings/workspace-connections.tsx');
    await render(WorkspaceConnections); await click(button('Manage access'));
    assert.match(document.querySelector('dialog').textContent, /default approver/);
    const select = document.querySelector('[aria-label="Update order delegation"]');
    await change(select, 'ask');
    await update(() => { button('Save access').click(); button('Save access').click(); });
    assert.equal(writes.length, 1); await update(() => release());
    assert.equal(select.value, 'ask');
    assert.match(document.querySelector('dialog').textContent, /Access changed/);
    assert.deepEqual(writes[0], { user: 'member', connection: 'connection', operation: 'order.update', ceiling: 'ask', revision: 0, connection_revision: 4 });
  }, { url, fetch: async (_, options = {}) => {
    if (options.body) { writes.push(JSON.parse(options.body)); await new Promise(resolve => { release = resolve; }); return { ok: false, status: 400, json: async () => ({ error: 'Access changed; reload' }) }; }
    return response(state({ connections: [connection] }));
  } });
});

test('creator checkbox changes require save and do not grant operations', async () => {
  const writes = [];
  await reactFixture(async ({ render, button, click }) => {
    const { WorkspaceConnections } = await import('../src/react/settings/workspace-connections.tsx');
    await render(WorkspaceConnections);
    await click(document.querySelector('input[type=checkbox]'));
    assert.equal(writes.length, 0);
    await click(button('Save agent creation for Priya'));
    assert.deepEqual(writes, [{ user: 'member', enabled: true, revision: 0 }]);
  }, { url, fetch: async (_, options = {}) => {
    if (options.body) { writes.push(JSON.parse(options.body)); return response({ saved: true }); }
    return response(state());
  } });
});

test('revoked workspace access clears account controls and a late response cannot repopulate a remount', async () => {
  let deny = false; let release;
  await reactFixture(async ({ render, button, click, update }) => {
    const { WorkspaceConnections } = await import('../src/react/settings/workspace-connections.tsx');
    await render(WorkspaceConnections, { key: 'owner' });
    assert.ok(button('Manage access')); deny = true;
    await click(button('Refresh connections'));
    assert.equal(button('Manage access'), undefined);
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

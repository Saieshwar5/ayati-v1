import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';
const workspace = '11111111-1111-4111-8111-111111111111';
const agent = { id: '22222222-2222-4222-8222-222222222222', name: 'Operations', editable: true };
const operation = { id: 'order.update', title: 'Update order', effect: 'write', schema: {} };
const connection = { id: 'connection', server: 'orders', enabled: true, revision: 1, definition: { id: 'orders', name: 'Orders', operations: [operation] } };
function state(extra = {}) { return { can_manage: true, viewer: 'owner', servers: [], connections: [connection], conversations: [{ id: 'chat', name: 'Operations' }], rules: [{ connection: connection.id, operation: operation.id, mode: 'block', revision: 3, approver_name: 'Owner', chat: 'chat' }], actions: [], ...extra }; }

test('control permissions retain a failed draft and synchronously guard repeated saves', async () => {
  const writes = []; let fail = true; let resolveWrite;
  await reactFixture(async ({ render, change, button, update, click }) => {
    const { AgentControl } = await import('../src/react/agents/control.tsx');
    await render(AgentControl, { agent, close() {} });
    await click(button('Permissions'));
    const select = document.querySelector('select[aria-label="Update order permission"]');
    await change(select, 'ask');
    assert.equal(writes.length, 0);
    await update(() => { button('Save permission').click(); button('Save permission').click(); });
    assert.equal(writes.length, 1);
    await update(() => resolveWrite());
    assert.match(document.querySelector('.ui-dialog-footer').textContent, /Save unavailable/);
    assert.equal(select.value, 'ask');
    fail = false; await click(button('Save permission')); await update(() => resolveWrite());
    assert.deepEqual(writes[0].body, { connection: 'connection', operation: 'order.update', mode: 'ask', revision: 3, chat: 'chat' });
    assert.deepEqual(writes[1].body, writes[0].body);
  }, { url: `http://localhost/w/${workspace}/?view=agents`, fetch: async (path, options = {}) => {
    if (options.method === 'POST') {
      writes.push({ path, body: JSON.parse(options.body) });
      await new Promise(resolve => { resolveWrite = resolve; });
      return { ok: !fail, json: async () => fail ? { error: 'Save unavailable' } : { saved: true } };
    }
    return { ok: true, json: async () => state() };
  } });
});

test('approval shows exact fields and retries the same decision after an unknown HTTP response', async () => {
  const writes = []; let fail = true;
  const action = { id: 'action', connection: 'connection', title: 'Update order', operation: 'order.update', effect: 'write', arguments: { order: 'one', address: '<script>private()</script>' }, state: 'pending', result: {}, chat: 'chat', thread: null, job: 'job', approver: 'owner', created: 1700000000, updated: 1700000000, requester_name: 'Alice', conversation_name: 'Sales' };
  await reactFixture(async ({ render, button, click }) => {
    const { AgentControl } = await import('../src/react/agents/control.tsx');
    await render(AgentControl, { agent, close() {} });
    assert.match(document.querySelector('dl').textContent, /<script>private/);
    assert.match(document.querySelector('.control-action').textContent, /Orders · Sales · Requested by Alice/);
    assert.equal(document.querySelector('script'), null);
    await click(button('Approve this action'));
    assert.match(document.querySelector('.ui-dialog-footer').textContent, /Response unavailable/);
    fail = false; await click(button('Approve this action'));
    assert.deepEqual(writes[0], writes[1]);
    assert.equal(writes[0].decision, 'approve');
    assert.match(writes[0].id, /^[a-f0-9-]{36}$/);
  }, { url: `http://localhost/w/${workspace}/?view=agents`, fetch: async (_, options = {}) => {
    if (options.method === 'POST') { writes.push(JSON.parse(options.body)); return { ok: !fail, json: async () => fail ? { error: 'Response unavailable' } : { ...action, state: 'completed' } }; }
    return { ok: true, json: async () => state({ actions: [action] }) };
  } });
});

test('read-only viewers cannot edit rules or approve another person’s action', async () => {
  await reactFixture(async ({ render, button, click }) => {
    const { AgentControl } = await import('../src/react/agents/control.tsx');
    await render(AgentControl, { agent: { ...agent, editable: false }, close() {} });
    await click(button('Permissions'));
    assert.equal(button('Save permission'), undefined);
    assert.equal(button('Connect system'), undefined);
    assert.equal(document.querySelector('select').disabled, true);
  }, { url: `http://localhost/w/${workspace}/?view=agents`, fetch: async () => ({ ok: true, json: async () => state({ can_manage: false, viewer: 'member' }) }) });
});

test('a late control response cannot populate another agent after remount', async () => {
  let release;
  await reactFixture(async ({ render, update }) => {
    const { AgentControl } = await import('../src/react/agents/control.tsx');
    await render(AgentControl, { key: 'first', agent, close() {} });
    await render(AgentControl, { key: 'second', agent: { ...agent, id: 'other', name: 'Other' }, close() {} });
    await update(() => release());
    assert.equal(document.querySelector('dialog').textContent.includes('Orders'), false);
    assert.match(document.querySelector('dialog').textContent, /Other/);
  }, { url: `http://localhost/w/${workspace}/?view=agents`, fetch: async path => {
    if (String(path).includes(agent.id)) { await new Promise(resolve => { release = resolve; }); return { ok: true, json: async () => state() }; }
    return { ok: true, json: async () => state({ connections: [] }) };
  } });
});

test('unsupported worker setup allows disconnection while preventing new tool permissions', async () => {
  const writes = [];
  await reactFixture(async ({ render, button, click }) => {
    const { AgentControl } = await import('../src/react/agents/control.tsx');
    await render(AgentControl, { agent, close() {} });
    await click(button('Permissions'));
    assert.match(document.querySelector('dialog').textContent, /Ask your operator to enable controller-managed execution/);
    assert.equal(button('Connect system'), undefined);
    const permission = document.querySelector('select[aria-label="Update order permission"]');
    assert.equal([...permission.options].find(option => option.value === 'allow').disabled, true);
    assert.equal([...permission.options].find(option => option.value === 'block').disabled, false);
    assert.equal(button('Disconnect').disabled, false);
    assert.equal(writes.length, 0);
  }, { url: `http://localhost/w/${workspace}/?view=agents`, fetch: async (_, options = {}) => {
    if (options.body) writes.push(JSON.parse(options.body));
    return { ok: true, json: async () => state({ execution_available: false }) };
  } });
});

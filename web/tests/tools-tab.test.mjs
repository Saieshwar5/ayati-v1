import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=agents`;
const agent = { id: 'atlas', name: 'Atlas', role: 'Orders', model: 'm', audience: 'everyone', revision: 1, settings: {}, editable: true };
const operation = (id, title, effect, workspace_mode, agent_mode = null) => ({ id, title, effect, workspace_mode, needs_other: false, agent_mode, mode: agent_mode ?? workspace_mode, revision: agent_mode ? 2 : 0 });
const connection = on => ({ id: 'books', server: 'books', name: 'Zoho Books', account: 'Accounts and orders', revision: 1, on, operations: [
  operation('order.read', 'Read orders', 'read', 'allow'),
  operation('order.update', 'Update an order', 'write', 'ask', 'ask'),
  operation('invoice.delete', 'Delete an invoice', 'write', 'block')] });
const controls = (extra = {}) => ({ can_manage: true, can_manage_connections: false, viewer: 'owner', connections: [connection(true)], actions: [], execution_available: true, ...extra });
const response = data => ({ ok: true, json: async () => data });

test('the switch and stricter rules save once, and looser choices cannot be picked', async () => {
  const writes = []; let release;
  await reactFixture(async ({ render, click, update }) => {
    const { ToolsTab } = await import('../src/react/agents/tools-tab.tsx');
    await render(ToolsTab, { agent });
    const books = document.querySelector('[aria-label="Zoho Books"]');
    assert.match(books.textContent, /admin limit: Ask first/);
    assert.match(books.textContent, /blocked by your admin/);
    assert.equal(books.querySelector('[aria-label="Update an order rule"] input[value=allow]').disabled, true);
    assert.equal(books.querySelector('[aria-label="Delete an invoice rule"] input[value=ask]').disabled, true);
    const block = books.querySelector('[aria-label="Update an order rule"] input[value=block]');
    await update(() => { block.click(); block.click(); });
    assert.equal(writes.length, 1);
    await update(() => release());
    assert.deepEqual(writes[0].body, { connection: 'books', operation: 'order.update', mode: 'block', revision: 2 });
    await click(books.querySelector('[role=switch]'));
    assert.ok(writes[1].path.endsWith('/control/tools'));
    assert.deepEqual(writes[1].body, { connection: 'books', enabled: false });
  }, { url, fetch: async (path, options = {}) => {
    if (options.body) {
      writes.push({ path, body: JSON.parse(options.body) });
      if (writes.length === 1) await new Promise(resolve => { release = resolve; });
      return response({ saved: true });
    }
    return response(controls());
  } });
});

test('people who cannot change the agent see its rules but no working controls', async () => {
  await reactFixture(async ({ render }) => {
    const { ToolsTab } = await import('../src/react/agents/tools-tab.tsx');
    await render(ToolsTab, { agent: { ...agent, editable: false } });
    assert.equal(document.querySelector('[role=switch]').disabled, true);
    assert.ok([...document.querySelectorAll('.mode-picker input')].every(input => input.disabled));
  }, { url, fetch: async () => response(controls({ can_manage: false })) });
});

test('connectors start off and say when tools are not enabled', async () => {
  await reactFixture(async ({ render }) => {
    const { ToolsTab } = await import('../src/react/agents/tools-tab.tsx');
    await render(ToolsTab, { agent });
    assert.equal(document.querySelector('[role=switch]').checked, false);
    assert.equal(document.querySelector('.mode-picker'), null);
    assert.match(document.body.textContent, /Business tools are not enabled/);
  }, { url, fetch: async () => response(controls({ connections: [connection(false)], execution_available: false })) });
});

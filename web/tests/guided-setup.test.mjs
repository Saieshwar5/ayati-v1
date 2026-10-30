import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
const workspace = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${workspace}/?view=agents`;
const agent = { id: 'agent', name: 'Orders assistant', scope: 'workspace', editable: true };
const operation = { id: 'order.update', title: 'Update order', effect: 'write', schema: { type: 'object', properties: { order: { type: 'string' } }, required: ['order'], additionalProperties: false } };
const account = { id: 'account', server: 'orders', name: 'Company orders', account: 'Northstar', permissions: [{ operation, ceiling: 'ask', approver: 'owner', approver_name: 'Ananya', delegation_revision: 2, rule_revision: 0 }] };
const context = { name: agent.name, responsibility: 'Handle order questions', accounts: [account], conversations: [{ id: 'chat', name: 'Order support' }] };
const draft = () => ({ id: 'draft', revision: 1, state: 'draft', previewed: false, receipt: null, data: { request: { id: 'draft', purpose: 'Handle order questions', chat: 'chat', connections: ['account'], assistant: false }, context, choices: [{ connection: 'account', operation: 'order.update', mode: 'block' }], inventories: [{ connection: 'account', tools: ['update_order', 'export_everything'], unsupported: ['export_everything'] }], notes: [] } });
const response = data => ({ ok: true, status: 200, json: async () => data });
const preview = revision => ({ revision, results: [{ connection: 'account', operation: 'order.update', decision: 'ask', approver: 'owner' }], example: null, business_operations_executed: 0 });

test('opening setup grants nothing; AI needs an explicit choice and a guarded creation action', async () => {
  const writes = []; let release;
  await reactFixture(async ({ render, button, click, update }) => {
    const { GuidedSetup } = await import('../src/react/agents/guided-setup.tsx');
    await render(GuidedSetup, { agent, close() {} });
    assert.equal(writes.length, 0);
    const ai = [...document.querySelectorAll('input[type=checkbox]')].at(-1);
    assert.equal(ai.checked, false);
    await click(document.querySelector('[aria-label="Use Company orders"]'));
    await click(ai);
    await update(() => { button('Create review draft').click(); button('Create review draft').click(); });
    assert.equal(writes.length, 1); await update(() => release());
    assert.equal(writes[0].assistant, true);
    assert.equal(writes[0].chat, 'chat'); assert.deepEqual(writes[0].connections, ['account']);
    assert.match(document.body.textContent, /1 reviewed action · 1 unsupported tool · 2 discovered tools/);
    assert.match(document.body.textContent, /export_everything/);
  }, { url, fetch: async (_, options = {}) => {
    if (options.body) { writes.push(JSON.parse(options.body)); await new Promise(resolve => { release = resolve; }); return response(draft()); }
    return response({ context, draft: null, execution_available: true });
  } });
});

test('a failed save retains choices and cannot activate or exceed the delegated limit', async () => {
  const writes = [];
  await reactFixture(async ({ render, button, change, click }) => {
    const { GuidedSetup } = await import('../src/react/agents/guided-setup.tsx');
    await render(GuidedSetup, { agent, close() {} });
    const select = document.querySelector('[aria-label="Company orders: Update order permission"]');
    assert.equal([...select.options].find(o => o.value === 'allow').disabled, true);
    assert.match(document.body.textContent, /Ask requests go to Ananya/);
    await change(select, 'ask'); await click(button('Save draft'));
    assert.equal(select.value, 'ask'); assert.match(document.body.textContent, /Save failed/);
    assert.equal(button('Activate reviewed permissions'), undefined);
    assert.deepEqual(writes[0], { revision: 1, choices: [{ connection: 'account', operation: 'order.update', mode: 'ask' }] });
  }, { url, fetch: async (_, options = {}) => {
    if (options.body) { writes.push(JSON.parse(options.body)); return { ok: false, status: 503, json: async () => ({ error: 'Save failed' }) }; }
    return response({ context, draft: draft(), execution_available: true });
  } });
});

test('saved preview and explicit review are required; an uncertain activation retries the exact payload', async () => {
  const writes = []; let fail = true;
  await reactFixture(async ({ render, button, click }) => {
    const { GuidedSetup } = await import('../src/react/agents/guided-setup.tsx');
    await render(GuidedSetup, { agent, close() {} });
    await click(button('Preview permissions'));
    assert.equal(button('Activate reviewed permissions').disabled, true);
    assert.match(document.body.textContent, /No business action runs/);
    await click(document.querySelector('input[type=checkbox]'));
    await click(button('Activate reviewed permissions'));
    assert.match(document.body.textContent, /Response unavailable/);
    fail = false; await click(button('Activate reviewed permissions'));
    const activations = writes.filter(w => w.path.endsWith('/activate'));
    assert.deepEqual(activations[0], activations[1]);
    assert.equal(activations[0].body.reviewed, true);
    assert.match(document.body.textContent, /Reviewed permissions activated/);
  }, { url, fetch: async (path, options = {}) => {
    if (options.body) {
      writes.push({ path, body: JSON.parse(options.body) });
      if (path.endsWith('/preview')) return response(preview(1));
      return fail ? { ok: false, status: 503, json: async () => ({ error: 'Response unavailable' }) } : response({ draft: 'draft', revision: 1, operations: 1, at: 1700000000 });
    }
    return response({ context, draft: draft(), execution_available: true });
  } });
});

test('stale authority blocks preview while retaining the saved review', async () => {
  await reactFixture(async ({ render, button, click }) => {
    const { GuidedSetup } = await import('../src/react/agents/guided-setup.tsx');
    await render(GuidedSetup, { agent, close() {} }); await click(button('Preview permissions'));
    assert.match(document.body.textContent, /Permissions changed/);
    assert.ok(document.querySelector('[aria-label="Company orders: Update order permission"]'));
    assert.equal(button('Activate reviewed permissions'), undefined);
  }, { url, fetch: async (_, options = {}) => options.body ? { ok: false, status: 400, json: async () => ({ error: 'Permissions changed. Start a new review' }) } : response({ context, draft: draft(), execution_available: true }) });
});

test('a late read cannot populate the next agent or grant access after closing setup', async () => {
  let release;
  await reactFixture(async ({ render, update }) => {
    const { GuidedSetup } = await import('../src/react/agents/guided-setup.tsx');
    await render(GuidedSetup, { key: 'old', agent, close() {} });
    await render(GuidedSetup, { key: 'new', agent: { ...agent, id: 'next', name: 'Next agent' }, close() {} });
    await update(() => release());
    assert.match(document.querySelector('dialog').textContent, /Next agent/);
    assert.equal(document.querySelector('[aria-label="Use Company orders"]'), null);
  }, { url, fetch: async path => {
    if (path.includes('/agent/')) { await new Promise(resolve => { release = resolve; }); return response({ context, draft: draft(), execution_available: true }); }
    return { ok: false, status: 400, json: async () => ({ error: 'Maintainer access required' }) };
  } });
});

test('AI suggestions prepared on an executor keep the review blocked until they arrive, and polling stops on close', async () => {
  const pending = () => { const d = draft(); d.data.request.assistant = true; d.data.suggestion = { state: 'pending', detail: 'Preparing AI suggestions. Every action stays blocked until you review it.' }; return d; };
  const ready = () => { const d = pending(); d.revision = 2; d.data.choices = [{ connection: 'account', operation: 'order.update', mode: 'ask' }]; d.data.suggestion = { state: 'ready', detail: 'AI suggestions are ready for your review.' }; return d; };
  let reads = 0, created = false, prepared = false;
  await reactFixture(async ({ render, button, click, update }) => {
    const { GuidedSetup } = await import('../src/react/agents/guided-setup.tsx');
    await render(GuidedSetup, { agent, close() {} });
    await click(document.querySelector('[aria-label="Use Company orders"]'));
    await click([...document.querySelectorAll('input[type=checkbox]')].at(-1));
    await update(() => button('Create review draft').click());
    assert.match(document.body.textContent, /Preparing AI suggestions/);
    assert.equal(button('Preview permissions').disabled, true);
    assert.equal(button('Save draft').disabled, true);
    prepared = true;
    await new Promise(resolve => setTimeout(resolve, 1700)); await update();
    assert.match(document.body.textContent, /AI suggestions are ready for your review/);
    assert.equal(button('Preview permissions').disabled, false);
    await render(() => null); // Closing the dialog unmounts the review.
    const seen = reads;
    await new Promise(resolve => setTimeout(resolve, 1700));
    assert.equal(reads, seen, 'Closing setup stops polling');
  }, { url, fetch: async (_, options = {}) => {
    if (options.body) { created = true; return response(pending()); }
    reads += 1;
    return response({ context, draft: !created ? null : prepared ? ready() : pending(), execution_available: true });
  } });
});

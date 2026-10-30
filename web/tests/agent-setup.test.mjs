import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
const workspace = '11111111-1111-4111-8111-111111111111';
const settings = { provider: '', instructions: '', escalation: '', avatar: 'spark', memory: false, browser: false, history: 'all', paused: false };
const agent = { id: '22222222-2222-4222-8222-222222222222', name: 'Mira', role: 'Purchasing', model: 'test', scope: 'private', revision: 0, settings, editable: true };
const directory = { role: 'Member', agents: [], default_model: 'test', system_prompt: 'System guidance', providers: [] };

test('agent setup preserves a failed draft and prevents double submission', async () => {
  const writes = []; let fail = true; let saved = 0;
  await reactFixture(async ({ render, button, click, change, update }) => {
    const { AgentEditor } = await import('../src/react/agents/editor.tsx');
    await render(AgentEditor, { agent, directory, creating: true, close() {}, saved: async () => { saved++; } });
    assert.deepEqual([...document.querySelector('select').options].map(o => o.value), ['private']);
    await change(document.querySelector('input'), 'Nova');
    await click(button('Continue')); await click(button('Continue')); await click(button('Continue'));
    assert.equal(writes.length, 0);
    await update(() => { button('Create agent').click(); button('Create agent').click(); });
    assert.equal(writes.length, 1); assert.equal(saved, 0);
    assert.match(document.querySelector('[role=status]').textContent, /Save unavailable/);
    assert.match(document.querySelector('dialog').textContent, /Nova/);
    fail = false; await click(button('Create agent'));
    assert.equal(saved, 1); assert.equal(writes.length, 2); assert.deepEqual(writes[0], writes[1]);
    assert.equal(writes[1].scope, 'private'); assert.equal(writes[1].settings.browser, false);
  }, { url: `http://localhost/w/${workspace}/?view=agents`, fetch: async (_, options) => {
    const body = JSON.parse(options.body); writes.push(body);
    return { ok: !fail, json: async () => fail ? { error: 'Save unavailable' } : { ...body, revision: 1 } };
  } });
});

test('existing agent visibility stays fixed and read-only viewers cannot save', async () => {
  const writes = [];
  await reactFixture(async ({ render, button }) => {
    const { AgentEditor } = await import('../src/react/agents/editor.tsx');
    await render(AgentEditor, { agent: { ...agent, revision: 1, editable: false }, directory, creating: false, close() {}, saved: async () => {} });
    assert.equal(document.querySelector('select').disabled, true);
    assert.equal(document.querySelector('input').matches(':disabled'), true);
    assert.equal(button('Save changes'), undefined); assert.equal(button('Add approved note'), undefined); assert.equal(writes.length, 0);
  }, { url: `http://localhost/w/${workspace}/?view=agents`, fetch: async (_, options = {}) => { if (options.body) writes.push(options.body); return { ok: true, json: async () => ({ notes: [] }) }; } });
});

test('forgetting knowledge uses its exact scope and revision', async () => {
  const note = { id: 'note', title: 'Supplier', content: 'Approved supplier', chat: 'private-channel', source_message: 'source', revision: 4, forgotten: false };
  const writes = [];
  await reactFixture(async ({ render, button, click }) => {
    const { Knowledge } = await import('../src/react/agents/knowledge.tsx');
    await render(Knowledge, { agent }); await click(button('Forget'));
    assert.deepEqual(writes, [{ ...note, forgotten: true }]);
  }, { url: `http://localhost/w/${workspace}/?view=agents`, fetch: async (_, options = {}) => { if (options.body) writes.push(JSON.parse(options.body)); return { ok: true, json: async () => ({ notes: writes.length ? [] : [note] }) }; } });
});

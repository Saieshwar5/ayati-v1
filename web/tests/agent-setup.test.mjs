import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
const workspace = '11111111-1111-4111-8111-111111111111';
const settings = { provider: '', instructions: '', escalation: '', avatar: 'spark', memory: false, browser: false, history: 'all', paused: false };
const agent = { id: '22222222-2222-4222-8222-222222222222', name: 'Mira', role: 'Purchasing', model: 'test', audience: 'only_me', revision: 0, settings, editable: true };
const directory = { role: 'Member', agents: [], default_model: 'test', system_prompt: 'System guidance', providers: [] };

test('read-only viewers cannot change who can talk to an agent or save', async () => {
  const writes = [];
  await reactFixture(async ({ render, button }) => {
    const { AgentEditor } = await import('../src/react/agents/editor.tsx');
    await render(AgentEditor, { agent: { ...agent, revision: 1, editable: false }, directory, close() {}, saved: async () => {} });
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

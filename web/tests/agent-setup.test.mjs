import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
const workspace = '11111111-1111-4111-8111-111111111111';
const settings = { provider: '', instructions: '', escalation: '', avatar: 'spark', memory: false, browser: false, history: 'all', paused: false };
const agent = { id: '22222222-2222-4222-8222-222222222222', name: 'Mira', role: 'Purchasing', model: 'test', audience: 'only_me', revision: 0, settings, editable: true };

test('read-only viewers read the instructions but cannot save them', async () => {
  const writes = [];
  await reactFixture(async ({ render, button }) => {
    const { InstructionsTab } = await import('../src/react/agents/instructions-tab.tsx');
    await render(InstructionsTab, { agent: { ...agent, revision: 1, editable: false, settings: { ...settings, instructions: 'Buy from approved suppliers.' } }, rules: 'System guidance', saved: async () => {} });
    assert.match(document.body.textContent, /Buy from approved suppliers\./);
    assert.equal(document.querySelector('textarea'), null);
    assert.equal(document.querySelector('select').disabled, true);
    assert.equal(button('Save changes'), undefined); assert.equal(button('Add approved note'), undefined); assert.equal(writes.length, 0);
  }, { url: `http://localhost/w/${workspace}/?view=agents`, fetch: async (_, options = {}) => { if (options.body) writes.push(options.body); return { ok: true, json: async () => ({ notes: [] }) }; } });
});

test('instructions save once with the revision the draft started from and keep a failed draft', async () => {
  const writes = []; let fail = true; let saved = 0;
  await reactFixture(async ({ render, button, change, click, update }) => {
    const { InstructionsTab } = await import('../src/react/agents/instructions-tab.tsx');
    const props = { agent: { ...agent, revision: 3, settings: { ...settings, instructions: 'Old' } }, rules: 'System guidance', saved: async () => { saved++; } };
    await render(InstructionsTab, props);
    assert.equal(button('Save changes').disabled, true);
    await change(document.querySelector('textarea'), 'New words');
    // A background refresh with someone else's change must not replace the draft or its revision.
    await render(InstructionsTab, { ...props, agent: { ...props.agent, revision: 4 } });
    assert.equal(document.querySelector('textarea').value, 'New words');
    await update(() => { button('Save changes').click(); button('Save changes').click(); });
    assert.equal(writes.length, 1); assert.equal(writes[0].revision, 3); assert.equal(saved, 0);
    assert.match(document.body.textContent, /Agent changed; reload before saving/);
    assert.equal(document.querySelector('textarea').value, 'New words');
    fail = false; await click(button('Save changes'));
    assert.equal(writes.length, 2); assert.equal(writes[1].settings.instructions, 'New words'); assert.equal(saved, 1);
    assert.equal(writes[1].people, undefined); assert.equal(writes[1].managers, undefined);
  }, { url: `http://localhost/w/${workspace}/?view=agents`, fetch: async (path, options = {}) => {
    if (path.endsWith('/knowledge')) return { ok: true, json: async () => ({ notes: [] }) };
    const body = JSON.parse(options.body); writes.push(body);
    return fail ? { ok: false, json: async () => ({ error: 'Agent changed; reload before saving' }) } : { ok: true, json: async () => ({ ...body, revision: body.revision + 1 }) };
  } });
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

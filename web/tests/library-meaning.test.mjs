import test from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
const workspace = '00000000-0000-4000-8000-000000000001';
const url = `http://localhost/w/${workspace}/?view=library`;
const response = (body, status = 200) => ({ ok: status < 400, status, json: async () => body });
const session = { user: { id: 'o', email: 'o@example.test' }, workspaces: [{ id: workspace, name: 'W', role: 'Admin' }] };
const folder = { id: 'policies', kind: 'folder', title: 'Policies' };

test('meaning search is off until the manager turns it on, after an agent proposed it', async () => {
  const sent = [];
  let info = { on: false, from_folder_above: false, available: true, can_change: true, proposals: [{ agent: 'Nova', reason: 'People ask in other words' }] };
  await reactFixture(async ({ render, click, button }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { MeaningControl } = await import('../src/react/library/meaning.tsx');
    await render(MeaningControl, { item: folder });
    assert.match(document.body.textContent, /Agents find what is here by its words\. Meaning search is off\./);
    assert.match(document.body.textContent, /Nova suggests turning on meaning search: People ask in other words/);
    await click(button('Turn on meaning search'));
    assert.deepEqual(sent, [{ on: true }]);
    assert.match(document.body.textContent, /Meaning search is on: .* sent to the model provider/);
    assert.equal(button('Dismiss'), undefined);
    await click(button('Turn off meaning search'));
    assert.deepEqual(sent[1], { on: false });
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (options.body) { const body = JSON.parse(options.body); sent.push(body); info = { ...info, on: body.on, proposals: [] }; return response({ ok: true }); }
    return response(info);
  } });
});

test('people who cannot change it see nothing while it is off, and a note while it is on', async () => {
  for (const [info, said] of [
    [{ on: false, from_folder_above: false, available: true, can_change: false, proposals: [] }, null],
    [{ on: false, from_folder_above: true, available: true, can_change: false, proposals: [] }, /on for a folder above this one/],
  ]) {
    await reactFixture(async ({ render }) => {
      const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
      const { MeaningControl } = await import('../src/react/library/meaning.tsx');
      await render(MeaningControl, { item: folder });
      if (said) assert.match(document.body.textContent, said); else assert.equal(document.body.textContent, '');
      assert.equal(document.querySelector('button'), null);
    }, { url, fetch: async path => path.endsWith('/session') ? response(session) : response(info) });
  }
});

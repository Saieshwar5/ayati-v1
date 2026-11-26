import test from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
const workspace = '00000000-0000-4000-8000-000000000001';
const url = `http://localhost/w/${workspace}/?view=library`;
const response = (body, status = 200) => ({ ok: status < 400, status, json: async () => body });
const session = { user: { id: 'o', email: 'o@example.test' }, workspaces: [{ id: workspace, name: 'W', role: 'Admin' }] };

test('an editor says what an item is and a failed save keeps the draft', async () => {
  const sent = [];
  let fail = true, changed = 0;
  await reactFixture(async ({ render, change, click, button }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { DescriptionControl } = await import('../src/react/library/description.tsx');
    await render(DescriptionControl, { item: { id: 'f', editable: true, description: '' }, changed: () => { changed += 1; } });
    assert.equal(button('Save description'), undefined, 'nothing to save yet');
    const input = document.querySelector('[aria-label="What this is"]');
    await change(input, 'Price list for desks and chairs');
    await click(button('Save description'));
    assert.match(document.querySelector('[role="alert"]').textContent, /Busy/);
    assert.equal(input.value, 'Price list for desks and chairs');
    assert.equal(changed, 0);
    fail = false;
    await click(button('Save description'));
    assert.deepEqual(sent.at(-1), { text: 'Price list for desks and chairs' });
    assert.equal(changed, 1);
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    sent.push(JSON.parse(options.body));
    return fail ? response({ error: 'Busy' }, 503) : response({ description: sent.at(-1).text });
  } });
});

test('someone who cannot edit reads the description and has no field', async () => {
  await reactFixture(async ({ render }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { DescriptionControl } = await import('../src/react/library/description.tsx');
    await render(DescriptionControl, { item: { id: 'g', editable: false, description: 'Board pack' }, changed() {} });
    assert.equal(document.body.textContent, 'Board pack');
    assert.equal(document.querySelector('input, button'), null);
  }, { url, fetch: async () => response(session) });
});

import test from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
const workspace = '00000000-0000-4000-8000-000000000001';
const url = `http://localhost/w/${workspace}/?view=agents`;
const response = (body, status = 200) => ({ ok: status < 400, status, json: async () => body });
const session = { user: { id: 'owner', email: 'o@example.test', display_name: 'Owner' }, workspaces: [{ id: workspace, name: 'W', role: 'Admin' }] };
const given = [
  { id: 'returns', title: 'Returns policy', folder: 'Policies', use_for: 'staff', characters: 3000 },
  { id: 'tone', title: 'Tone guide', folder: null, use_for: 'viewers', characters: 6000 },
];

test('managers choose, order and save the documents an agent always reads', async () => {
  const sent = [];
  let fail = true;
  await reactFixture(async ({ render, change, click, update, button }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { AlwaysRead } = await import('../src/react/library/always-read.tsx');
    await render(AlwaysRead, { agent: { id: 'nova', name: 'Nova' } });
    assert.match(document.body.textContent, /None yet/);
    for (const id of ['returns', 'tone']) {
      await change(document.querySelector('[aria-label="Add a document"]'), id);
      await click(button('Add'));
    }
    assert.match(document.body.textContent, /9,000 of 8,000 characters · the end of the list is cut short/);
    await click(button('Move Tone guide up'));
    assert.deepEqual([...document.querySelectorAll('.library-always li')].map(li => li.textContent.split('Policies')[0].split('People')[0]), ['Tone guide', 'Returns policy']);
    await update(() => { button('Save documents').click(); button('Save documents').click(); });
    assert.equal(sent.length, 1, 'one save per click burst');
    assert.deepEqual(sent[0], { items: ['tone', 'returns'], expected: [] });
    assert.match(document.querySelector('[role=status]').textContent, /changed/);
    assert.equal(document.querySelectorAll('.library-always li').length, 2, 'the draft is kept after a failure');
    fail = false;
    await click(button('Save documents'));
    assert.match(document.querySelector('[role=status]').textContent, /Saved/);
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    assert.ok(path.endsWith('/library-always/nova'));
    if (!options.body) return response({ items: fail ? [] : [given[1], given[0]], given, hidden: 0, chosen: fail ? 0 : 2, editable: true, most: 5, budget: 8000 });
    sent.push(JSON.parse(options.body));
    return fail ? response({ error: 'This item changed. Reload the saved version before trying again; your draft is preserved.' }, 409) : response({ items: ['tone', 'returns'] });
  } });
});

test('people who do not manage the agent read the list and are told about documents they cannot open', async () => {
  await reactFixture(async ({ render }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { AlwaysRead } = await import('../src/react/library/always-read.tsx');
    await render(AlwaysRead, { agent: { id: 'nova', name: 'Nova' } });
    assert.match(document.body.textContent, /Returns policy/);
    assert.match(document.body.textContent, /1 more you can't open/);
    assert.equal(document.querySelector('[aria-label="Add a document"]'), null);
    assert.equal([...document.querySelectorAll('button')].some(b => b.textContent === 'Remove'), false);
  }, { url, fetch: async path => path.endsWith('/session') ? response(session) : response({ items: [given[0]], given: [given[0]], hidden: 1, chosen: 2, editable: false, most: 5, budget: 8000 }) });
});

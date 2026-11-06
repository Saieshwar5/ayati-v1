import test from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
const workspace = '00000000-0000-4000-8000-000000000001';
const url = `http://localhost/w/${workspace}/?view=library&item=t`;
const response = (body, status = 200) => ({ ok: status < 400, status, json: async () => body });
const session = { user: { id: 'o', email: 'o@example.test' }, workspaces: [{ id: workspace, name: 'W', role: 'Admin' }] };
const table = { type: 'table', columns: [{ name: 'Product', type: 'text' }, { name: 'Price', type: 'number' }], rows: [['Standing desk', 1250], ['Chair', 300]] };
const item = editable => ({ id: 't', kind: 'table', title: 'Price list', revision: 4, editable, document: table });

test('pasted rows become columns, with numbers where every value is a number', async () => {
  const { parseTable } = await import('../src/react/library/table-view.tsx');
  assert.deepEqual(parseTable('Product,Price,Note\nDesk,1250,big\nChair,,\n'), {
    type: 'table', columns: [{ name: 'Product', type: 'text' }, { name: 'Price', type: 'number' }, { name: 'Note', type: 'text' }],
    rows: [['Desk', 1250, 'big'], ['Chair', null, null]],
  });
  assert.equal(parseTable('A\tB\nx\t1').columns[1].type, 'number', 'tab-separated rows from a spreadsheet');
});

test('editors change cells and rows and save the whole table once from the revision they read', async () => {
  const sent = [];
  let saved = 0, fail = true;
  await reactFixture(async ({ render, change, click, update, button }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { TableView } = await import('../src/react/library/table-view.tsx');
    await render(TableView, { item: item(true), saved: () => { saved++; } });
    assert.equal(button('Save table').disabled, true);
    await change(document.querySelector('[aria-label="Price, row 2"]'), '320');
    await click(button('Add row'));
    await change(document.querySelector('[aria-label="Product, row 3"]'), 'Lamp');
    await click(button('Delete row 1'));
    await update(() => { button('Save table').click(); button('Save table').click(); });
    assert.equal(sent.length, 1, 'one save per click burst');
    assert.deepEqual(sent[0].table.rows, [['Chair', 320], ['Lamp', null]]);
    assert.equal(sent[0].revision, 4);
    assert.match(document.querySelector('[role=status]').textContent, /changed/);
    assert.equal(document.querySelector('[aria-label="Product, row 2"]').value, 'Lamp', 'the draft is kept');
    fail = false;
    await click(button('Save table'));
    assert.equal(sent[1].request, sent[0].request, 'the retry keeps its request');
    assert.equal(saved, 1);
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    assert.ok(path.endsWith('/library-tables'));
    sent.push(JSON.parse(options.body));
    return fail ? response({ error: 'This item changed. Reload the saved version before trying again; your draft is preserved.' }, 409) : response({ id: 't', revision: 5 });
  } });
});

test('everyone with access reads and filters a table without editing it', async () => {
  await reactFixture(async ({ render, change }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { TableView } = await import('../src/react/library/table-view.tsx');
    await render(TableView, { item: item(false), saved: () => {} });
    assert.equal(document.querySelector('.library-grid input'), null);
    assert.equal([...document.querySelectorAll('button')].some(b => b.textContent === 'Save table'), false);
    await change(document.querySelector('[aria-label="Filter rows"]'), 'chair');
    assert.deepEqual([...document.querySelectorAll('.library-grid tbody tr')].map(r => r.textContent), ['Chair300']);
    assert.match(document.body.textContent, /the first column is the key agents look up/);
  }, { url, fetch: async () => response(session) });
});

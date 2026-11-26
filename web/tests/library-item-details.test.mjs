import test from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
const workspace = '00000000-0000-4000-8000-000000000001';
const url = `http://localhost/w/${workspace}/?view=library`;
const response = (body, status = 200) => ({ ok: status < 400, status, json: async () => body });
const session = { user: { id: 'o', email: 'o@example.test' }, workspaces: [{ id: workspace, name: 'W', role: 'Member' }] };
const item = { id: 'p', kind: 'page', title: 'Price list', revision: 4, updated: 1_700_000_000, visibility: 'workspace', editable: true, manageable: true,
  folder: null, description: 'Agreed prices', agents: [{ agent: 'nova', level: 'edit', use_for: 'viewers' }], working: ['Nova'] };

test('one panel holds what an item is, who can use it, its history and what is happening to it', async () => {
  let shared = 0, history = 0;
  await reactFixture(async ({ render, click }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { ItemDetails } = await import('../src/react/library/item-details.tsx');
    await render(ItemDetails, { item, refresh() {}, share: () => { shared += 1; }, history: () => { history += 1; } });
    const tab = name => [...document.querySelectorAll('[role="tab"]')].find(t => t.textContent === name);
    const panel = () => document.querySelector('[role="tabpanel"]');
    assert.deepEqual([...document.querySelectorAll('[role="tab"]')].map(t => t.textContent), ['About', 'Sharing', 'History', 'Activity']);
    assert.equal(tab('About').getAttribute('aria-selected'), 'true');
    assert.equal(panel().querySelector('[aria-label="What this is"]').value, 'Agreed prices');
    await click(tab('Sharing'));
    assert.match(panel().textContent, /Workspace.*this item.*Can edit · For people who can see it/);
    await click([...panel().querySelectorAll('button')].find(b => b.textContent === 'Share'));
    await click(tab('History'));
    assert.match(panel().textContent, /Revision 4/);
    await click([...panel().querySelectorAll('button')].find(b => b.textContent === 'Version history'));
    await click(tab('Activity'));
    assert.match(panel().textContent, /Nova is working on this now/);
    assert.deepEqual([shared, history], [1, 1]);
  }, { url, fetch: async path => path.endsWith('/session') ? response(session)
    : response({ review: null, keep_days: null, folder_keep_days: null, deletes_at: null, hold: null, can_change: false, fields: null }) });
});

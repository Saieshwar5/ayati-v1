import test from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
const workspace = '00000000-0000-4000-8000-000000000001';
const url = `http://localhost/w/${workspace}/?view=library`;
const response = (body, status = 200) => ({ ok: status < 400, status, json: async () => body });
const session = { user: { id: 'o', email: 'o@example.test' }, workspaces: [{ id: workspace, name: 'W', role: 'Admin' }] };
const inbox = { id: 'f', kind: 'folder', title: 'Inbox', agents: [{ agent: 'iris', level: 'suggest', use_for: 'staff' }] };

test('a folder manager asks an agent given the folder to handle new items, then stops it', async () => {
  const sent = [];
  let watcher = null;
  await reactFixture(async ({ render, change, click, button }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { FolderWatch } = await import('../src/react/library/watching.tsx');
    await render(FolderWatch, { item: inbox });
    await click(button('Watch with an agent'));
    const options = [...document.querySelectorAll('[aria-label="Agent"] option')].map(o => o.textContent);
    assert.deepEqual(options, ['Choose an agent given this folder', 'Iris'], 'only agents given the folder');
    assert.equal(button('Save').disabled, true);
    await change(document.querySelector('[aria-label="Agent"]'), 'iris');
    await change(document.querySelector('[aria-label="What should it do with each new item?"]'), 'File invoices and ask me whether to pay.');
    await click(button('Save'));
    assert.deepEqual(sent[0], { agent: 'iris', instructions: 'File invoices and ask me whether to pay.' });
    assert.match(document.body.textContent, /Iris handles new items here for O: File invoices and ask me whether to pay\. · 1 waiting/);
    assert.match(document.querySelector('a').getAttribute('href'), /view=chat&conversation=c1$/);
    await click(button('Stop watching'));
    assert.deepEqual(sent[1], { agent: null, instructions: '' });
    assert.ok(button('Watch with an agent'));
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (path.endsWith('/chat-directory')) return response({ users: [], agents: [{ id: 'iris', name: 'Iris' }, { id: 'nova', name: 'Nova' }] });
    if (options.body) { const body = JSON.parse(options.body); sent.push(body); watcher = body.agent ? { agent: 'iris', agent_name: 'Iris', instructions: body.instructions, chat: 'c1', by: 'O', waiting: 1 } : null; return response({ ok: true }); }
    return response({ watcher, can_change: true });
  } });
});

test('people see who watches a folder and an item\'s recorded fields, without controls', async () => {
  await reactFixture(async ({ render }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { FolderWatch, ItemFields } = await import('../src/react/library/watching.tsx');
    await render(() => null);
    await render(FolderWatch, { item: inbox });
    assert.match(document.body.textContent, /Iris handles new items here for O/);
    assert.equal(document.querySelector('button'), null);
    await render(ItemFields, { item: { id: 'p', kind: 'page', title: 'Invoice 42', agents: [] } });
    const fields = [...document.querySelectorAll('.library-fields dt')].map(d => d.textContent);
    assert.deepEqual(fields, ['amount', 'due date']);
    assert.match(document.querySelector('.library-fields').textContent, /1250.*2026-12-01.*Recorded by Iris/);
  }, { url, fetch: async path => path.endsWith('/session') ? response(session)
    : path.endsWith('/watch') ? response({ watcher: { agent: 'iris', agent_name: 'Iris', instructions: 'File invoices.', chat: null, by: 'O', waiting: 0 }, can_change: false })
    : response({ fields: { fields: { amount: 1250, due_date: '2026-12-01' }, agent: 'Iris', at: 1_700_000_000 } }) });
});

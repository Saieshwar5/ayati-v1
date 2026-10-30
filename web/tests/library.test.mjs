import test from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { reactFixture } from './react-fixture.mjs';
const workspace = '00000000-0000-4000-8000-000000000001';
const url = `http://localhost/w/${workspace}/?view=library`;
const response = (body, status = 200) => ({ ok: status < 400, status, json: async () => body });
const doc = text => ({ type: 'doc', content: [{ type: 'paragraph', content: [{ type: 'text', text }] }] });
const item = { id: 'page', title: 'Plan', document: doc('Original'), revision: 1, access_revision: 1, editable: true, manageable: true, people: [], agents: [], visibility: 'private' };
const session = { user: { id: 'writer', email: 'writer@example.test', display_name: 'Writer' }, workspaces: [{ id: workspace, name: 'Workspace', role: 'Member' }] };

test('Library sharing lets a User select existing people and separates agent read access', async () => {
  let payload, resolveWrite;
  await reactFixture(async ({ render, update, change, click, button }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { ShareDialog } = await import('../src/react/library/dialogs.tsx');
    await render(ShareDialog, { item, close: () => {}, changed: () => {} });
    await change(document.querySelector('select'), 'selected');
    await change(document.querySelector('[aria-label="Access for Reader"]'), 'view');
    await click(document.querySelector('input[type=checkbox]'));
    await update(() => { button('Save changes').click(); button('Save changes').click(); });
    assert.equal(payload.visibility, 'selected');
    assert.deepEqual(payload.people, [{ user: 'reader', access: 'view' }]);
    assert.deepEqual(payload.agents, ['support']);
    await update(() => resolveWrite(response({ error: 'Access changed' }, 403)));
    assert.match(document.querySelector('[role=status]').textContent, /Access changed/);
    assert.equal(document.querySelector('[aria-label="Access for Reader"]').value, 'view');
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (path.endsWith('/chat-directory')) return response({ users: [{ id: 'writer', name: 'Writer' }, { id: 'reader', name: 'Reader' }] });
    if (path.endsWith('/agent-profiles')) return response({ agents: [{ id: 'support', name: 'Support', scope: 'workspace' }, { id: 'private', name: 'Private', scope: 'private' }] });
    assert.ok(path.endsWith('/library/page/share'));
    assert.equal(payload, undefined, 'double submission is blocked synchronously');
    payload = JSON.parse(options.body); return new Promise(resolve => { resolveWrite = resolve; });
  } });
});

test('page save preserves failed drafts and blocks stale overwrite retries', async () => {
  let writes = 0;
  await reactFixture(async ({ render, update, click, button }) => {
    const { enterWorkspace, storageScope } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { usePageSave } = await import('../src/react/library/autosave.ts');
    function Harness() {
      const save = usePageSave(item);
      return createElement('div', null,
        createElement('button', { onClick: () => save.change('My draft', doc('Keep this text')) }, 'Edit'),
        createElement('button', { disabled: save.blocked, onClick: () => void save.save() }, 'Save'),
        createElement('p', { role: 'status' }, save.status));
    }
    await render(Harness); await click(button('Edit')); await click(button('Save'));
    assert.equal(writes, 1); assert.match(document.querySelector('[role=status]').textContent, /changed/);
    assert.equal(button('Save').disabled, true);
    const stored = JSON.parse(localStorage.getItem(storageScope('cuckoo.library.draft.page')));
    assert.equal(stored.title, 'My draft'); assert.equal(stored.document.content[0].content[0].text, 'Keep this text');
    await update(); assert.equal(writes, 1);
  }, { url, fetch: async path => {
    if (path.endsWith('/session')) return response(session);
    writes++; return response({ error: 'This item changed. Draft preserved.' }, 409);
  } });
});

test('uncertain page saves retry the same payload with the same request identity', async () => {
  const bodies = [];
  await reactFixture(async ({ render, click, button }) => {
    const { enterWorkspace, storageScope } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { usePageSave } = await import('../src/react/library/autosave.ts');
    function Harness() {
      const save = usePageSave(item);
      return createElement('div', null,
        createElement('button', { onClick: () => save.change('Updated', doc('Saved text')) }, 'Edit'),
        createElement('button', { onClick: () => void save.save() }, 'Save'),
        createElement('p', { role: 'status' }, save.status));
    }
    await render(Harness); await click(button('Edit')); await click(button('Save'));
    assert.match(document.querySelector('[role=status]').textContent, /Connection/);
    await click(button('Save'));
    assert.deepEqual(bodies[0], bodies[1]);
    assert.match(document.querySelector('[role=status]').textContent, /All changes saved/);
    assert.equal(localStorage.getItem(storageScope('cuckoo.library.draft.page')), null);
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    bodies.push(JSON.parse(options.body));
    if (bodies.length === 1) throw new Error('Connection interrupted');
    return response({ ...item, revision: 2 });
  } });
});

import test from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
const workspace = '00000000-0000-4000-8000-000000000001';
const url = `http://localhost/w/${workspace}/?view=library`;
const response = (body, status = 200) => ({ ok: status < 400, status, json: async () => body });
const session = { user: { id: 'owner', email: 'owner@example.test', display_name: 'Owner' }, workspaces: [{ id: workspace, name: 'Workspace', role: 'Admin' }] };
const folder = { id: 'sales', kind: 'folder', title: 'Sales', revision: 1, access_revision: 2, editable: true, manageable: true, people: [], agents: [], visibility: 'private' };
const directory = path => {
  if (path.endsWith('/session')) return response(session);
  if (path.endsWith('/chat-directory')) return response({ users: [{ id: 'owner', name: 'Owner' }, { id: 'ravi', name: 'Ravi' }] });
  if (path.endsWith('/agent-profiles')) return response({ agents: [{ id: 'nova', name: 'Nova', audience: 'workspace' }] });
};

test('a folder is shared with an agent to answer all staff, and people can edit what is in it', async () => {
  let payload;
  await reactFixture(async ({ render, change, click, button }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { ShareDialog } = await import('../src/react/library/dialogs.tsx');
    await render(ShareDialog, { item: folder, close: () => {}, changed: () => {} });
    assert.match(document.body.textContent, /Everything in this folder follows these settings/);
    await change(document.querySelector('[aria-label="People access"]'), 'selected');
    const ravi = document.querySelector('[aria-label="Access for Ravi"]');
    assert.ok([...ravi.options].some(o => o.value === 'edit'), 'folders offer Can edit');
    await change(ravi, 'edit');
    assert.equal(document.querySelector('[aria-label="Use for, Nova"]'), null, 'no audience until the agent is chosen');
    await click(document.querySelector('input[type=checkbox]'));
    const useFor = document.querySelector('[aria-label="Use for, Nova"]');
    assert.equal(useFor.value, 'viewers', 'the safest audience is the default');
    assert.deepEqual([...useFor.options].map(o => o.textContent), ['People who can see it', 'All staff']);
    await change(useFor, 'staff');
    await click(button('Save changes'));
    assert.deepEqual(payload.agents, [{ agent: 'nova', level: 'read', use_for: 'staff' }]);
    assert.deepEqual(payload.people, [{ user: 'ravi', access: 'edit' }]);
    assert.equal(payload.revision, 2);
  }, { url, fetch: async (path, options = {}) => directory(path) ?? (assert.ok(path.endsWith('/library/sales/share')), payload = JSON.parse(options.body), response({ ok: true })) });
});

test('a new folder is created once, keeps its identity on retry and opens', async () => {
  const sent = [];
  let fail = true;
  await reactFixture(async ({ render, change, update, button, dom }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { NewFolderDialog } = await import('../src/react/library/folders.tsx');
    await render(NewFolderDialog, { close: () => {} });
    assert.equal(button('Create folder').disabled, true);
    await change(document.querySelector('[aria-label="Folder name"]'), 'Policies');
    await update(() => { button('Create folder').click(); button('Create folder').click(); });
    assert.equal(sent.length, 1, 'double submission is blocked');
    assert.match(document.body.textContent, /Storage busy/);
    assert.equal(document.querySelector('[aria-label="Folder name"]').value, 'Policies', 'the name is kept');
    fail = false;
    await update(() => button('Create folder').click());
    assert.equal(sent.length, 2);
    assert.equal(sent[1].id, sent[0].id); assert.equal(sent[1].request, sent[0].request);
    assert.deepEqual(Object.keys(sent[0]).sort(), ['id', 'request', 'title']);
    assert.equal(sent[0].title, 'Policies');
    assert.ok(dom.window.location.search.endsWith(`folder=${sent[0].id}`), 'the new folder opens');
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    assert.ok(path.endsWith(`/workspaces/${workspace}/library-folders`));
    sent.push(JSON.parse(options.body));
    return fail ? response({ error: 'Storage busy' }, 503) : response({ id: sent.at(-1).id });
  } });
});

test('moving offers only folders the person can edit and sends the sharing revision', async () => {
  let moved;
  const page = { ...folder, id: 'notes', kind: 'page', title: 'Notes', access_revision: 4, folder: null };
  await reactFixture(async ({ render, change, click, button }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { MoveDialog } = await import('../src/react/library/folders.tsx');
    await render(MoveDialog, { item: page, close: () => {}, changed: () => {} });
    const choices = [...document.querySelector('[aria-label="Folder"]').options].map(o => o.textContent);
    assert.deepEqual(choices, ['Not in a folder', 'Sales']);
    assert.equal(button('Move').disabled, true, 'nothing to move yet');
    await change(document.querySelector('[aria-label="Folder"]'), 'sales');
    await click(button('Move'));
    assert.equal(moved.folder, 'sales'); assert.equal(moved.revision, 4); assert.ok(moved.request);
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (path.includes('/library?view=all')) return response({ items: [folder, { ...folder, id: 'hr', title: 'HR', editable: false }, { ...page, id: 'x' }], more: false, available: true, storage: 'local' });
    assert.ok(path.endsWith('/library/notes/move')); moved = JSON.parse(options.body); return response({ ok: true });
  } });
});

test('an open folder lists its own items and says its sharing applies to them', async () => {
  const listed = [];
  await reactFixture(async ({ render }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { LibraryPage } = await import('../src/react/library/library-page.tsx');
    await render(LibraryPage, { href: `${url}&folder=sales` });
    assert.ok(listed.some(path => path.includes('folder=sales')));
    assert.equal(document.querySelector('.library-folder-header h2').textContent, 'Sales');
    assert.match(document.querySelector('.library-folder-header').textContent, /Workspace · 1 agent \(all staff\)\. Pages and files here follow the folder's sharing\./);
    assert.ok(document.querySelector('nav[aria-label="Folder path"] a'));
    assert.equal([...document.querySelectorAll('button')].some(b => b.textContent === '＋ New folder'), false, 'one level of folders');
    assert.equal(document.querySelector('.library-name a').textContent, 'Quote template');
  }, { url: `${url}&folder=sales`, fetch: async path => {
    if (path.endsWith('/session')) return response(session);
    if (path.endsWith('/library/sales')) return response({ ...folder, visibility: 'workspace', agents: [{ agent: 'nova', level: 'read', use_for: 'staff' }] });
    listed.push(path);
    return response({ items: [{ ...folder, id: 'quote', kind: 'page', title: 'Quote template', folder: 'sales', folder_title: 'Sales', updated: 1 }], more: false, available: true, storage: 'local' });
  } });
});

import test from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
const workspace = '00000000-0000-4000-8000-000000000001';
const url = `http://localhost/w/${workspace}/?view=library`;
const response = (body, status = 200) => ({ ok: status < 400, status, json: async () => body });
const session = { user: { id: 'o', email: 'o@example.test' }, workspaces: [{ id: workspace, name: 'W', role: 'Admin' }] };

test('admins set a folder keeping rule and put it on legal hold, with a reason', async () => {
  const sent = [];
  let info = { keep_days: null, folder_keep_days: null, deletes_at: null, hold: null, can_change: true };
  await reactFixture(async ({ render, change, click, button }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { KeepingControl } = await import('../src/react/library/keeping.tsx');
    await render(KeepingControl, { item: { id: 'f', kind: 'folder', title: 'ID checks' } });
    await change(document.querySelector('[aria-label="Keep items"]'), '90');
    assert.deepEqual(sent[0], { path: 'keeping', body: { keep_days: 90 } });
    assert.equal(document.querySelector('[aria-label="Keep items"]').value, '90');
    await click(button('Legal hold…'));
    assert.equal(button('Put on hold').disabled, true, 'needs a reason');
    await change(document.querySelector('[aria-label="Why"]'), 'Tribunal');
    await click(button('Put on hold'));
    assert.deepEqual(sent[1], { path: 'hold', body: { reason: 'Tribunal' } });
    assert.match(document.body.textContent, /On legal hold since .*: Tribunal\. It can't be trashed or deleted/);
    await click(button('Lift legal hold'));
    assert.deepEqual(sent[2], { path: 'hold', body: { reason: null } });
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (options.body) {
      const body = JSON.parse(options.body); sent.push({ path: path.split('/').at(-1), body });
      if ('keep_days' in body) info = { ...info, keep_days: body.keep_days };
      else info = { ...info, hold: body.reason ? { reason: body.reason, at: 1_700_000_000, by: 'O', on_folder: false } : null };
      return response({ ok: true });
    }
    return response(info);
  } });
});

test('people see when an item will be deleted, or that its folder holds it, without controls', async () => {
  for (const [info, said] of [
    [{ keep_days: null, folder_keep_days: 30, deletes_at: 1_900_000_000, hold: null, can_change: false }, /Deleted for good on .* under its folder's keeping rule/],
    [{ keep_days: null, folder_keep_days: 30, deletes_at: 1_900_000_000, hold: { reason: 'Audit', at: 1_700_000_000, by: 'O', on_folder: true }, can_change: false }, /On legal hold with its folder since .*: Audit/],
  ]) {
    await reactFixture(async ({ render }) => {
      const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
      const { KeepingControl } = await import('../src/react/library/keeping.tsx');
      await render(KeepingControl, { item: { id: 'p', kind: 'page', title: 'Photo' } });
      assert.match(document.body.textContent, said);
      assert.equal(document.querySelector('button, select'), null);
    }, { url, fetch: async path => path.endsWith('/session') ? response(session) : response(info) });
  }
});

test('creators delete trashed items for good, one by one or by emptying Trash', async () => {
  const posted = [];
  let items = [
    { id: 'a', kind: 'page', title: 'Old minutes', visibility: 'private', revision: 2, manageable: true, trashed: true, agent_count: 0, updated: 1, mime: '', size: 0 },
    { id: 'b', kind: 'page', title: 'Old draft', visibility: 'private', revision: 2, manageable: true, trashed: true, agent_count: 0, updated: 1, mime: '', size: 0 },
  ];
  await reactFixture(async ({ render, click, button }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { LibraryPage } = await import('../src/react/library/library-page.tsx');
    await render(LibraryPage, { href: url });
    await click(button('Trash'));
    await click(button('Delete Old minutes for good'));
    assert.deepEqual(posted[0], '/library/a/delete-forever');
    assert.deepEqual([...document.querySelectorAll('.library-name strong')].map(n => n.textContent), ['Old draft']);
    await click(button('Empty Trash'));
    assert.equal(posted[1], '/library-trash/empty');
    assert.match(document.body.textContent, /Deleted 1 item for good\./);
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (options.body) {
      const short = path.replace(`/api/identity/workspaces/${workspace}`, '');
      posted.push(short);
      if (short.endsWith('/delete-forever')) { items = items.filter(i => i.id !== 'a'); return response({ deleted: 1 }); }
      if (short.endsWith('/empty')) { const n = items.length; items = []; return response({ deleted: n }); }
      return response({ ok: true });
    }
    return response({ items: path.includes('view=trash') ? items : [], more: false, available: true, storage: 'local' });
  } });
});

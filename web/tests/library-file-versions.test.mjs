import test from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
const workspace = '00000000-0000-4000-8000-000000000001';
const url = `http://localhost/w/${workspace}/?view=library`;
const response = (body, status = 200) => ({ ok: status < 400, status, json: async () => body });
const session = { user: { id: 'o', email: 'o@example.test' }, workspaces: [{ id: workspace, name: 'W', role: 'Admin' }] };
const file = { id: 'f', kind: 'file', title: 'Price list.txt', revision: 3, editable: true, mime: 'text/plain', size: 10 };

test('an editor replaces a file from the revision read; a failed transfer retries as the same upload', async () => {
  const started = [], bytes = [];
  let fail = true, changed = 0;
  await reactFixture(async ({ render, update, dom }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { ReplaceFile } = await import('../src/react/library/file-versions.tsx');
    await render(ReplaceFile, { item: file, changed: () => { changed += 1; } });
    const input = document.querySelector('[aria-label="New version of this file"]');
    const choose = async () => {
      Object.defineProperty(input, 'files', { configurable: true, value: [new File(['Desk: 1250'], 'new.txt')] });
      await update(() => input.dispatchEvent(new dom.window.Event('change', { bubbles: true })));
      for (let i = 0; i < 5; i++) { await new Promise(r => setTimeout(r, 10)); await update(); }
    };
    await choose();
    assert.match(document.querySelector('[role="alert"]').textContent, /Storage busy/);
    assert.equal(changed, 0, 'nothing changed after a failure');
    fail = false;
    await choose();
    assert.equal(started.length, 2);
    assert.deepEqual(started[0].replaces, { item: 'f', revision: 3 });
    assert.equal(started[0].size, 10);
    assert.equal(started[0].checksum.length, 64);
    assert.deepEqual([started[1].id, started[1].request], [started[0].id, started[0].request], 'same upload on retry');
    assert.deepEqual(bytes, [started[0].id, started[0].id]);
    assert.equal(changed, 1);
    assert.equal(document.querySelector('[role="alert"]'), null);
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (path.endsWith('/bytes')) { bytes.push(path.split('/').at(-2)); return fail ? response({ error: 'Storage busy' }, 503) : response({ id: 'f' }); }
    started.push(JSON.parse(options.body)); return response({ id: started.at(-1).id });
  } });
});

test('file history offers each version to download and restores one as the next revision', async () => {
  const posted = [];
  await reactFixture(async ({ render, click, button }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { HistoryDialog } = await import('../src/react/library/dialogs.tsx');
    await render(HistoryDialog, { item: file, close() {}, changed() {} });
    assert.match(document.body.textContent, /last 20 versions of this file/);
    const links = [...document.querySelectorAll('a.ui-button')].map(a => a.getAttribute('href'));
    assert.deepEqual(links, [3, 2].map(v => `/api/identity/workspaces/${workspace}/library/f/download?version=${v}`));
    assert.equal(button('View'), undefined, 'a file has no text preview');
    const restore = [...document.querySelectorAll('button')].filter(b => b.textContent === 'Restore');
    assert.equal(restore[0].disabled, true, 'the current version');
    await click(restore[1]);
    assert.equal(posted[0].action, 'restore_version');
    assert.deepEqual([posted[0].revision, posted[0].version], [3, 2]);
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (options.body) { posted.push(JSON.parse(options.body)); return response({ ok: true }); }
    return response({ versions: [
      { revision: 3, title: 'Price list.txt', author: 'O', created: 1_700_000_100, size: 10 },
      { revision: 2, title: 'Price list.txt', author: 'O', agent: 'Nova', created: 1_700_000_000, size: 9 },
    ] });
  } });
});

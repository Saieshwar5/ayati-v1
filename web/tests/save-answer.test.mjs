import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=chat&conversation=chat`;
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const session = response({ user: { id: 'owner', email: 'owner@example.test' }, workspaces: [{ id: w, name: 'Store', role: 'Admin' }] });
const answer = { id: 'm2', chat: 'chat', author: 'owner', kind: 'user', name: 'Asha', content: 'Visitors park in **bay C**.', state: 'sent', at: 1, sequence: 2 };
const listing = { items: [{ id: 'f1', kind: 'folder', title: 'Policies', editable: true }, { id: 'f2', kind: 'folder', title: 'HR', editable: false }, { id: 'p1', kind: 'page', title: 'Returns', editable: true }], more: false, available: true, storage: 'local' };

test('A message is saved to a Library folder from its menu, once per click burst; a failure keeps the draft', async () => {
  const saves = [];
  let fail = true;
  await reactFixture(async ({ render, click, change, update, button }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { ChatMessage } = await import('../src/react/chat/message.tsx');
    await render(ChatMessage, { message: answer, directory: { users: [], agents: [], role: 'Admin' } });
    await click(button('More actions'));
    await click(button('Save to Library'));
    const dialog = document.querySelector('dialog[open]');
    assert.equal(dialog.querySelector('[aria-label="Answer"]').value, 'Visitors park in **bay C**.');
    assert.deepEqual([...dialog.querySelectorAll('[aria-label="Folder"] option')].map(o => o.textContent), ['No folder (only you)', 'Policies'], 'only folders they may add to');
    assert.equal(button('Save').disabled, true, 'needs a title');
    await change(dialog.querySelector('[aria-label="Question or title"]'), 'Where do visitors park?');
    await change(dialog.querySelector('[aria-label="Folder"]'), 'f1');
    await update(() => { button('Save').click(); button('Save').click(); });
    assert.equal(saves.length, 1, 'one save per click burst');
    assert.match(dialog.textContent, /Library is unavailable/);
    assert.equal(dialog.querySelector('[aria-label="Question or title"]').value, 'Where do visitors park?', 'the draft stays');
    await click(button('Save'));
    assert.equal(saves.length, 2);
    assert.equal(saves[1].id, saves[0].id, 'a retry is the same page');
    assert.deepEqual({ ...saves[1], id: undefined, request: undefined }, { id: undefined, request: undefined, folder: 'f1', title: 'Where do visitors park?', text: 'Visitors park in **bay C**.', message: 'm2' });
    assert.match(dialog.textContent, /Agents given this folder can answer it from now on/);
    assert.match(dialog.querySelector('a').getAttribute('href'), new RegExp(`view=library&item=${saves[0].id}$`));
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return session;
    if (path.includes('/library?')) return response(listing);
    if (path.endsWith('/library-answers')) { saves.push(JSON.parse(options.body)); if (fail) { fail = false; return response({ error: 'Library is unavailable' }, 503); } return response({ revision: 1 }); }
    return response({});
  } });
});

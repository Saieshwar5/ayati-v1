import test from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
const workspace = '00000000-0000-4000-8000-000000000001';
const url = `http://localhost/w/${workspace}/?view=library`;
const response = (body, status = 200) => ({ ok: status < 400, status, json: async () => body });
const session = { user: { id: 'owner', email: 'o@example.test', display_name: 'Owner' }, workspaces: [{ id: workspace, name: 'W', role: 'Admin' }] };
const suggestion = (id, stale = false) => ({ id, base_revision: 3, text: `Suggested ${id}`, note: 'Ravi asked', created: 1, agent: 'Nova', requester: 'Ravi', stale });

test('a page shows agents\' suggestions; accepting one sends one decision and reloads the page', async () => {
  const decisions = [];
  let reloaded = 0, fail = true;
  await reactFixture(async ({ render, click, update, button }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { Suggestions } = await import('../src/react/library/suggestions.tsx');
    await render(Suggestions, { id: 'page', revision: 3, changed: () => { reloaded++; } });
    assert.match(document.querySelector('.library-suggestion-bar').textContent, /2 suggested changes from agents/);
    await click(button('Review'));
    const fresh = document.querySelector('[aria-label="Suggestion from Nova"]');
    assert.match(fresh.textContent, /for Ravi/);
    assert.match(fresh.textContent, /“Ravi asked”/);
    assert.match(fresh.textContent, /Now.*Current text.*Suggested.*Suggested one/s);
    const stale = document.querySelectorAll('.library-suggestion')[1];
    assert.equal([...stale.querySelectorAll('button')].find(b => b.textContent === 'Accept').disabled, true, 'a stale suggestion can only be rejected');
    const accept = [...fresh.querySelectorAll('button')].find(b => b.textContent === 'Accept');
    await update(() => { accept.click(); accept.click(); });
    assert.equal(decisions.length, 1, 'one decision per click burst');
    assert.equal(reloaded, 0, 'a failed decision changes nothing');
    fail = false;
    await click([...document.querySelector('[aria-label="Suggestion from Nova"]').querySelectorAll('button')].find(b => b.textContent === 'Accept'));
    assert.equal(decisions.length, 2);
    assert.equal(decisions[1].request, decisions[0].request, 'the retry keeps its request identity');
    assert.equal(decisions[1].action, 'accept');
    assert.equal(reloaded, 1);
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (path.endsWith('/page/suggestions')) return response({ suggestions: [suggestion('one'), suggestion('two', true)], current: 'Current text', editable: true });
    assert.ok(path.endsWith('/page/suggestions/one'));
    decisions.push(JSON.parse(options.body));
    return fail ? response({ error: 'Busy' }, 503) : response({ state: 'accepted', revision: 4 });
  } });
});

test('people who cannot edit see suggestions without deciding them; no suggestions, no bar', async () => {
  let none = false;
  await reactFixture(async ({ render, click, button }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { Suggestions } = await import('../src/react/library/suggestions.tsx');
    await render(Suggestions, { id: 'page', revision: 3, changed: () => {} });
    await click(button('Review'));
    assert.match(document.body.textContent, /Someone who can edit this page decides/);
    assert.equal([...document.querySelectorAll('button')].some(b => b.textContent === 'Accept'), false);
  }, { url, fetch: async path => path.endsWith('/session') ? response(session) : response({ suggestions: [suggestion('one')], current: 'Now', editable: false }) });
  await reactFixture(async ({ render }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { Suggestions } = await import('../src/react/library/suggestions.tsx');
    await render(Suggestions, { id: 'page', revision: 3, changed: () => {} });
    assert.equal(document.querySelector('.library-suggestion-bar'), null);
    none = true;
  }, { url, fetch: async path => path.endsWith('/session') ? response(session) : response({ suggestions: [], current: '', editable: true }) });
  assert.ok(none);
});

test('folders offer every agent level, pages no "save here", files only reading', async () => {
  const levels = async kind => {
    let options;
    await reactFixture(async ({ render, click }) => {
      const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
      const { ShareDialog } = await import('../src/react/library/dialogs.tsx');
      await render(ShareDialog, { item: { id: 'x', kind, title: 'X', revision: 1, access_revision: 1, people: [], agents: [], visibility: 'private', manageable: true }, close: () => {}, changed: () => {} });
      await click(document.querySelector('input[type=checkbox]'));
      const select = document.querySelector('[aria-label="What Nova can do"]');
      options = select ? [select.value, ...[...select.options].map(o => o.textContent)] : null;
    }, { url, fetch: async path => {
      if (path.endsWith('/session')) return response(session);
      if (path.endsWith('/chat-directory')) return response({ users: [] });
      return response({ agents: [{ id: 'nova', name: 'Nova', audience: 'workspace' }] });
    } });
    return options;
  };
  assert.deepEqual(await levels('folder'), ['suggest', 'Read', 'Suggest edits', 'Edit', 'Edit and save here']);
  assert.deepEqual(await levels('page'), ['suggest', 'Read', 'Suggest edits', 'Edit']);
  assert.equal(await levels('file'), null);
});

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

// Memory (schema 142): Settings › Memory for admins, an agent's Memory for its
// owners, and About you for everyone who talks to an agent.
const workspace = '11111111-1111-4111-8111-111111111111';
const base = `/api/identity/workspaces/${workspace}`;
const response = (value, status = 200) => ({ ok: status < 400, status, json: async () => value });
const options = fetch => ({ url: `http://localhost/w/${workspace}/?view=settings&section=memory`, fetch });
const counts = { notes: 3, topics: 1, essentials: 1, fading: 0, archive: 1, waiting: 1 };
const note = (id, text, extra = {}) => ({ id, scope: 'workspace', kind: 'customer', topic: 'Acme Traders', text, source: 'agent', by: 'Nova',
  room: null, learned_in: { id: 'sales', name: 'sales', kind: 'channel' }, created_ms: 1794700000000, used: 4, used_ms: 1794700000000,
  strength: 80, pinned: false, essential: false, until_ms: null, state: 'active', waiting_reason: null, replaces: null, ...extra });
const waiting = note('w1', 'Acme Traders pays 60 days after invoice.', { state: 'waiting', waiting_reason: 'disagreement', learned_in: null,
  replaces: { id: 'n1', text: 'Acme Traders pays 45 days after invoice.', used: 41, created_ms: 1, pinned: false } });
const page = (view, items) => ({ available: true, view, items, counts, used_month: 212, summaries: false });
const topic = { topic: 'Acme Traders', notes: 3, used: 41, updated_ms: 1794700000000, strength: 80, pinned: false, summary: null, lead: 'Acme Traders pays 45 days after invoice.' };

test('admins answer a correction once, see topics, open a topic and the notes refresh', async () => {
  const writes = []; let kept = false; let finish;
  await reactFixture(async ({ render, button, click, update }) => {
    const { MemoryBrowser } = await import('../src/react/memory/memory-browser.tsx');
    await render(MemoryBrowser, {});
    assert.match(document.body.textContent, /3 notes/);
    assert.match(document.body.textContent, /used 212 times in 30 days/);
    const needs = document.querySelector('section[aria-label="Needs your answer"]');
    assert.match(needs.textContent, /Corrects a note/);
    assert.match(needs.textContent, /Now in memory, used 41 times.*45 days/);
    assert.match(needs.textContent, /in a conversation you can’t open/);
    assert.match(document.body.textContent, /Acme Traders pays 45 days after invoice\./, 'a topic shows its strongest note without a summary');
    await update(() => { button('Replace it').click(); button('Replace it').click(); });
    assert.equal(writes.length, 1, 'one answer for a double click');
    assert.equal(writes[0].path, `${base}/memory/notes/w1/keep`);
    kept = true;
    await update(() => finish(response({ ok: true })));
    assert.match(document.body.textContent, /Kept, and the old note ended\./);
    assert.equal(document.querySelector('section[aria-label="Needs your answer"]'), null);
    await click(button('Open Acme Traders'));
    assert.match(document.body.textContent, /Notes about Acme Traders/);
    assert.match(document.body.textContent, /Nova learned it in sales/);
    assert.match(document.body.textContent, /Strong/);
  }, options(async (path, request = {}) => {
    if (request.body) { writes.push({ path, body: JSON.parse(request.body) }); return new Promise(resolve => { finish = resolve; }); }
    const view = new URL(path, 'http://x').searchParams.get('view');
    if (view === 'waiting') return response(page('waiting', kept ? [] : [waiting]));
    if (view === 'notes') {
      assert.equal(new URL(path, 'http://x').searchParams.get('topic'), 'Acme Traders');
      return response(page('notes', [note('n2', 'Acme Traders pays 60 days after invoice.')]));
    }
    return response(page('topics', [topic]));
  }));
});

test('removing asks first, a failed change keeps the note and says why, and members are refused', async () => {
  const writes = [];
  await reactFixture(async ({ render, button, click }) => {
    const { MemoryBrowser } = await import('../src/react/memory/memory-browser.tsx');
    await render(MemoryBrowser, { agent: 'nova' });
    await click(button('Notes3'));
    await click(button('Remove…'));
    assert.match(document.body.textContent, /Remove for good\?/);
    assert.equal(writes.length, 0, 'nothing is removed before the answer');
    await click(button('Keep it'));
    await click(button('No longer true'));
    assert.equal(writes[0], `${base}/agent-profiles/nova/memory/notes/n1/end`);
    assert.match(document.querySelector('[role=status].ui-status').textContent, /That note is not in use/);
    assert.match(document.body.textContent, /Acme Traders pays 45 days after invoice\./);
  }, options(async (path, request = {}) => {
    if (request.body) { writes.push(path); return response({ error: 'That note is not in use' }, 400); }
    const view = new URL(path, 'http://x').searchParams.get('view');
    if (view === 'notes') return response(page('notes', [note('n1', 'Acme Traders pays 45 days after invoice.')]));
    return response(page(view, []));
  }));
  await reactFixture(async ({ render }) => {
    const { MemoryBrowser } = await import('../src/react/memory/memory-browser.tsx');
    await render(MemoryBrowser, {});
    assert.match(document.querySelector('[role=alert]').textContent, /Only admins can open the workspace’s memory/);
  }, options(async () => response({ error: 'Only admins can open the workspace’s memory' }, 403)));
});

test('a new note keeps its id when retried after a failure, and its form clears once saved', async () => {
  const writes = []; let fail = true;
  await reactFixture(async ({ render, button, click, change, submit }) => {
    const { MemoryBrowser } = await import('../src/react/memory/memory-browser.tsx');
    await render(MemoryBrowser, {});
    await click(button('Add a note'));
    const form = document.querySelector('form[aria-label="Add a note"]');
    await change(form.querySelector('input'), '  Ravi approves   discounts over 10%. ');
    await change(form.querySelector('select'), 'people');
    await submit(form);
    assert.match(document.body.textContent, /The server is busy/);
    fail = false;
    await submit(form);
    assert.equal(writes.length, 2);
    assert.equal(writes[0].id, writes[1].id, 'the retry reuses the id');
    assert.deepEqual({ ...writes[1], id: undefined }, { id: undefined, kind: 'people', topic: '', text: 'Ravi approves discounts over 10%.', until: null });
    assert.equal(document.querySelector('form[aria-label="Add a note"]'), null);
    assert.match(document.body.textContent, /Saved\./);
  }, options(async (path, request = {}) => {
    if (request.body) { writes.push(JSON.parse(request.body)); return fail ? response({ error: 'The server is busy' }, 503) : response({ ok: true }); }
    return response(page('topics', []));
  }));
});

test('About you lists only the viewer’s notes and removes one', async () => {
  const writes = []; let removed = false;
  await reactFixture(async ({ render, button, click }) => {
    const { AboutMe } = await import('../src/react/memory/about-me.tsx');
    await render(AboutMe, { agent: 'nova', name: 'Nova' });
    assert.match(document.body.textContent, /Only you can see them/);
    await click(button('Remove: Meera wants short answers.'));
    assert.deepEqual(writes, [`${base}/agent-profiles/nova/memory/about-me/m1/remove`]);
    assert.match(document.body.textContent, /Nothing yet\. When Nova learns/);
  }, options(async (path, request = {}) => {
    if (request.body) { writes.push(path); removed = true; return response({ ok: true }); }
    assert.equal(path, `${base}/agent-profiles/nova/memory/about-me`);
    return response({ available: true, notes: removed ? [] : [note('m1', 'Meera wants short answers.', { scope: 'person', kind: 'preference' })] });
  }));
});

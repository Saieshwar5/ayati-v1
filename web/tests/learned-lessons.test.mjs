import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

// Knowledge → Lessons from its work (schema 121): proposed lessons are approved,
// edited or rejected by the agent's maintainers; nothing changes before that.
const workspace = '11111111-1111-4111-8111-111111111111';
const agent = { id: 'agent', name: 'Nova', role: 'Help', model: 'fixture', audience: 'selected',
  revision: 1, editable: true, settings: { memory: true, history: 'since_join' } };
const response = (value, status = 200) => ({ ok: status < 400, status, json: async () => value });
const options = fetch => ({ url: `http://localhost/w/${workspace}/?view=agents`, fetch });
const lesson = (id, title, extra = {}) => ({ id, title, content: 'When: A customer asks about delivery\nDo: Give the date first',
  why: 'People asked again', kind: 'new', state: 'proposed', created_ms: 1, conversation: { id: 'ops', name: 'Operations' },
  replaces: null, evidence: [{ kind: 'answer', excerpt: 'It goes by BlueDart.', conversation: 'Operations' }, { kind: 'reaction' }], ...extra });
const learning = proposals => ({ available: true, configured: true, enabled: true, revision: 2,
  last_run_ms: 1794700000000, last_result: '2 lessons to review', proposals });

test('a lesson is approved for its own conversation unless widened, once, and the notes refresh', async () => {
  const writes = []; let saved = 0; let finish;
  await reactFixture(async ({ render, button, click, update }) => {
    const { LearnedLessons } = await import('../src/react/agents/learned-lessons.tsx');
    await render(LearnedLessons, { agent, saved: async () => { saved += 1; } });
    assert.match(document.body.textContent, /2 lessons to review/);
    const article = document.querySelector('article[aria-label="Delivery date first"]');
    assert.match(article.textContent, /A person answered its reply in Operations: “It goes by BlueDart.”/);
    assert.match(article.textContent, /A person reacted to its reply in a conversation you can’t open/);
    const only = [...article.querySelectorAll('label')].find(l => l.textContent === 'Only in Operations').querySelector('input');
    assert.equal(only.checked, true, 'its own conversation by default');
    await update(() => { button('Approve').click(); button('Approve').click(); });
    assert.equal(writes.length, 1, 'one decision for a double click');
    assert.deepEqual(writes[0], { path: `/api/identity/workspaces/${workspace}/agent-profiles/agent/learning/l1`, body: { approve: true, everywhere: false } });
    await update(() => finish(response({ id: 'l1', state: 'approved' })));
    assert.equal(saved, 1);
    assert.match(document.body.textContent, /“Delivery date first” is now part of Nova’s knowledge/);
  }, options(async (path, request = {}) => {
    if (request.body) { writes.push({ path, body: JSON.parse(request.body) }); return new Promise(resolve => { finish = resolve; }); }
    return response(learning(writes.length ? [lesson('l1', 'Delivery date first', { state: 'approved' })] : [lesson('l1', 'Delivery date first')]));
  }));
});

test('an edited lesson is approved with its new wording, an update shows the note it changes, and a rejection keeps nothing', async () => {
  const writes = [];
  const update = lesson('l2', 'Delivery questions', { kind: 'update', conversation: null,
    replaces: { id: 'n1', title: 'Delivery questions', content: 'Name the courier.' } });
  await reactFixture(async ({ render, button, click, change }) => {
    const { LearnedLessons } = await import('../src/react/agents/learned-lessons.tsx');
    await render(LearnedLessons, { agent, saved: async () => {} });
    const changes = document.querySelector('article[aria-label="Delivery questions"]');
    assert.match(changes.textContent, /Updates “Delivery questions”/);
    assert.match(changes.textContent, /Now: Name the courier\./);
    assert.equal(changes.querySelector('fieldset'), null, 'an update keeps its note’s audience');
    const first = document.querySelector('article[aria-label="Delivery date first"]');
    await click([...first.querySelectorAll('button')].find(b => b.textContent === 'Edit'));
    await change(document.querySelector('form textarea'), 'When a customer asks about delivery, give the date first.');
    await click(button('Approve edited lesson'));
    assert.deepEqual(writes[0].body, { approve: true, everywhere: false, title: 'Delivery date first', content: 'When a customer asks about delivery, give the date first.' });
    const again = document.querySelector('article[aria-label="Delivery questions"]');
    await click([...again.querySelectorAll('button')].find(b => b.textContent === 'Reject'));
    assert.deepEqual(writes[1].body, { approve: false, everywhere: false });
  }, options(async (path, request = {}) => {
    if (request.body) { writes.push({ path, body: JSON.parse(request.body) }); return response({ state: 'ok' }); }
    return response(learning([lesson('l1', 'Delivery date first'), update]));
  }));
});

test('maintainers can switch learning off, and an unset Cuckoo says so', async () => {
  const writes = [];
  await reactFixture(async ({ render, click }) => {
    const { LearnedLessons } = await import('../src/react/agents/learned-lessons.tsx');
    await render(LearnedLessons, { agent, saved: async () => {} });
    assert.match(document.body.textContent, /Learning isn’t set up on this Cuckoo yet/);
    assert.match(document.body.textContent, /No lessons waiting/);
    await click(document.querySelector('input[role=switch]'));
    assert.deepEqual(writes[0], { enabled: false, revision: 2 });
    assert.equal(document.querySelector('input[role=switch]').checked, false);
  }, options(async (path, request = {}) => {
    if (request.body) { const body = JSON.parse(request.body); writes.push(body); return response({ ...learning([]), configured: false, enabled: body.enabled, revision: 3 }); }
    return response({ ...learning([]), configured: false });
  }));
});

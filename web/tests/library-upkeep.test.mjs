import test from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
const workspace = '00000000-0000-4000-8000-000000000001';
const url = `http://localhost/w/${workspace}/?view=library`;
const response = (body, status = 200) => ({ ok: status < 400, status, json: async () => body });
const session = { user: { id: 'o', email: 'o@example.test' }, workspaces: [{ id: workspace, name: 'W', role: 'Admin' }] };

test('editors set a review interval; an overdue document says so until marked reviewed', async () => {
  const sent = [];
  let review = { every_days: 90, reviewed: 1, due: 1000, overdue: true };
  await reactFixture(async ({ render, change, click, button }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { ReviewControl } = await import('../src/react/library/upkeep.tsx');
    await render(ReviewControl, { item: { id: 'p', editable: true } });
    assert.match(document.querySelector('.library-review').textContent, /Due for review since/);
    await click(button('Mark reviewed'));
    assert.deepEqual(sent[0], { every_days: null, reviewed: true });
    assert.match(document.querySelector('.library-review').textContent, /Reviewed every 90 days/);
    await change(document.querySelector('[aria-label="Review every"]'), '');
    assert.deepEqual(sent[1], { every_days: null, reviewed: false });
    assert.match(document.querySelector('.library-review').textContent, /No review date/);
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (options.body) { const body = JSON.parse(options.body); sent.push(body); review = body.reviewed ? { ...review, overdue: false, due: 99999999999 } : null; return response({ ok: true }); }
    return response({ review });
  } });
});

test('readers see the review date without changing it', async () => {
  await reactFixture(async ({ render }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { ReviewControl } = await import('../src/react/library/upkeep.tsx');
    await render(ReviewControl, { item: { id: 'p', editable: false } });
    assert.match(document.body.textContent, /Reviewed every 30 days/);
    assert.equal(document.querySelector('select'), null);
  }, { url, fetch: async path => path.endsWith('/session') ? response(session) : response({ review: { every_days: 30, reviewed: 1, due: 99999999999, overdue: false } }) });
});

test('admins answer an unanswered question with a new page, once, or dismiss it', async () => {
  const pages = [], closed = [];
  await reactFixture(async ({ render, click, update, button, dom }) => {
    const { enterWorkspace } = await import('../src/workspace-context.ts'); await enterWorkspace();
    const { UnansweredQuestions } = await import('../src/react/library/upkeep.tsx');
    await render(UnansweredQuestions);
    assert.match(document.body.textContent, /Parking at the office\?.*Asked 3 times · Nova/);
    assert.doesNotMatch(document.body.textContent, /Ravi/, 'who asked is not shown');
    await update(() => { button('Write the answer').click(); button('Write the answer').click(); });
    assert.equal(pages.length, 1, 'one page per click burst');
    assert.equal(pages[0].title, 'Parking at the office?');
    assert.deepEqual(closed, [{ id: 'g1', action: 'answered' }]);
    assert.match(dom.window.location.search, new RegExp(`item=${pages[0].id}`));
  }, { url, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (path.endsWith('/library-gaps')) return response({ gaps: [{ id: 'g1', query: 'Parking at the office?', asked: 3, last_at: 1, agent: 'Nova' }] });
    if (path.includes('/library-gaps/')) { closed.push({ id: path.split('/').at(-1), action: JSON.parse(options.body).action }); return response({ state: 'answered' }); }
    pages.push(JSON.parse(options.body)); return response({ id: pages[0].id, revision: 1 });
  } });
});

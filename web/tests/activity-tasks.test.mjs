import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { reactFixture } from './react-fixture.mjs';

// Tasks in Activity and Chat (people-and-agents-first, Phase 2): Open, a task's
// details and closing it, Make this a task, the chip, and lighter chat rows.
const w = '11111111-1111-4111-8111-111111111111';
const openUrl = `http://localhost/w/${w}/?view=activity&tab=open`;
const session = { user: { id: 'kavya', email: 'kavya@example.test' }, workspaces: [{ id: w, name: 'Store', role: 'Member' }] };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const now = Date.now();
const task = (id, title, status, extra = {}) => ({ id, title, kind: 'commitment', status, origin: 'agent', done_when: null, waiting_on: null,
  agent: 'nova', agent_name: 'Nova', agent_avatar: null, requester: 'kavya', requester_name: 'Kavya Nair', chat: 'sales', conversation_name: 'sales',
  conversation_kind: 'channel', created_ms: now, start_ms: null, due_ms: null, closed_ms: null, outcome: null, result: null, steps: 1, ...extra });
const details = (extra = {}) => ({ ...task('t-1', 'Send quotes to Acme', 'open', { done_when: 'Acme has both quotes' }), may_close: false,
  requests: [{ id: 'r-1', state: 'complete', created_ms: now, summary: 'Prepare quotes for Acme' }], ...extra });
const common = path => {
  if (path.endsWith('/session')) return response(session);
  if (path.includes('/attention')) return response({ available: true, items: [], next_before: null });
};
async function activity() {
  const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
  return (await import('../src/react/activity/activity-page.tsx')).ActivityPage;
}

test('Open lists what needs a person first and a task closes as not a task once for a double click', async () => {
  const reads = [], writes = [];
  await reactFixture(async ({ render, button, click, update }) => {
    await render(await activity(), { href: openUrl });
    assert.match(reads.find(p => p.includes('/activity/tasks?')), /scope=mine/);
    const headings = [...document.querySelectorAll('.activity-open h2')].map(h => h.textContent);
    assert.deepEqual(headings, ['Needs fixing · 1', 'Waiting · 1', 'Open · 1']);
    assert.match(document.body.textContent, /waiting for an approval/);
    await click([...document.querySelectorAll('.activity-row-button')].find(b => b.textContent.includes('Send quotes to Acme')));
    const drawer = document.querySelector('dialog');
    assert.match(drawer.textContent, /Done when.*Acme has both quotes/);
    assert.match(drawer.textContent, /Prepare quotes for Acme/);
    assert.equal(button('Mark done'), undefined, 'only the person it is for, managers and admins');
    await update(() => { const node = button('Not a task'); node.click(); node.click(); });
    assert.equal(writes.length, 1);
    assert.ok(writes[0].path.endsWith('/activity/tasks/t-1/close'));
    assert.deepEqual(JSON.parse(writes[0].body), { outcome: 'not_task' });
  }, { url: openUrl, fetch: async (path, options = {}) => {
    reads.push(path);
    const shared = common(path); if (shared) return shared;
    if (options.method === 'POST') { writes.push({ path, body: options.body }); return response({ id: 't-1', outcome: 'not_task' }); }
    if (path.includes('/activity/tasks/t-1')) return response(details());
    if (path.includes('/activity/tasks')) return response({ more: false, items: [
      task('t-2', 'Book the courier', 'needs_fixing'), task('t-3', 'Refund order 123', 'waiting', { kind: 'automatic', waiting_on: 'approval' }),
      task('t-1', 'Send quotes to Acme', 'open')] });
  } });
});

test('a link to a task opens it in Open, and its owner can mark it done', async () => {
  const writes = [];
  await reactFixture(async ({ render, button, click }) => {
    await render(await activity(), { href: `${openUrl}&task=t-1` });
    assert.match(document.querySelector('dialog').textContent, /Send quotes to Acme/);
    await click(button('Mark done'));
    assert.deepEqual(JSON.parse(writes[0]), { outcome: 'done' });
  }, { url: `${openUrl}&task=t-1`, fetch: async (path, options = {}) => {
    const shared = common(path); if (shared) return shared;
    if (options.method === 'POST') { writes.push(options.body); return response({ id: 't-1', outcome: 'done' }); }
    if (path.includes('/activity/tasks/t-1')) return response(details({ may_close: true }));
    return response({ more: false, items: [task('t-1', 'Send quotes to Acme', 'open')] });
  } });
});

test('request details offer Make this a task once, or say which task the request is part of', async () => {
  const writes = [];
  const request = (extra = {}) => ({ id: 'r-9', chat: 'sales', thread: null, conversation_name: 'sales', conversation_kind: 'channel', in_conversation: true,
    agent: 'nova', agent_name: 'Nova', agent_avatar: null, kind: 'chat', requester: 'kavya', requester_name: 'Kavya Nair', summary: 'Check stock',
    state: 'complete', latest_update: '', created_ms: now, finished_ms: now, can_cancel: false, approvals: [], message: 'Check stock', reply: 'Done',
    steps: [], fix: null, disagreement: null, ...extra });
  for (const [extra, expectButton] of [[{}, true], [{ task: { id: 't-1', title: 'Send quotes to Acme', kind: 'commitment', status: 'open' } }, false]]) {
    await reactFixture(async ({ render, button, update }) => {
      const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
      const { RequestDetails } = await import('../src/react/activity/request-details.tsx');
      await render(RequestDetails, { reply: 'r-9', close: () => {} });
      if (expectButton) {
        await update(() => { const node = button('Make this a task'); node.click(); node.click(); });
        assert.equal(writes.length, 1);
        assert.ok(writes[0].endsWith('/activity/requests/r-9/task'));
      } else {
        assert.equal(button('Make this a task'), undefined);
        assert.match(document.querySelector('.activity-task-line').textContent, /Part of task Send quotes to Acme · Open/);
      }
    }, { url: openUrl, fetch: async (path, options = {}) => {
      const shared = common(path); if (shared) return shared;
      if (options.method === 'POST') { writes.push(path); return response({ id: 't-9', title: 'Check stock' }); }
      return response(request(extra));
    } });
  }
});

test('the chat chip shows promises, and tracked work only when it needs a look or is done', async () => {
  const { TaskChip } = await import('../src/react/chat/task-chip.tsx');
  for (const [taskRef, text] of [
    [{ id: 't-1', title: 'Send quotes to Acme', kind: 'commitment', status: 'open' }, /Task: Send quotes to Acme · Open/],
    [{ id: 't-2', title: 'Refund order 123', kind: 'automatic', status: 'done' }, /Tracked: Refund order 123 · Done/],
    [{ id: 't-3', title: 'Update order', kind: 'automatic', status: 'working' }, null],
  ]) {
    await reactFixture(async ({ render }) => {
      await render(() => createElement(TaskChip, { task: taskRef }));
      const chip = document.querySelector('.task-chip');
      if (!text) assert.equal(chip, null);
      else { assert.match(chip.textContent, text); assert.match(chip.href, /tab=open&task=/); }
    }, { url: openUrl });
  }
});

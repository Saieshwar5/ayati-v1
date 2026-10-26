import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';

const workspace = '11111111-1111-4111-8111-111111111111';
const conversations = [{ id: 'a', name: 'First private chat' }, { id: 'b', name: 'Second private chat' }];
const value = (conversation, content) => ({ mode: 'current_preview', agent: 'agent', conversation,
  assembled_at: 1000, history_policy: 'since_join', message_limit: 60, knowledge_enabled: true,
  paused: false, archived: false, messages: [{ id: conversation, at: 1000, content,
    author: { id: 'person', kind: 'user', display_name: 'Alex' } }], notes: [] });
const response = (body, status = 200) => ({ ok: status < 400, status, json: async () => body });
const props = { agent: 'agent', conversations, conversationError: '', close: () => {} };
const options = fetch => ({ url: `http://localhost/w/${workspace}/?view=agents`, fetch });

test('context preview makes no request until a conversation is chosen and ignores stale responses from another conversation', async () => {
  const reads = []; let finishA;
  await reactFixture(async ({ render, change, update }) => {
    const { ContextPreviewDialog } = await import('../src/react/agents/context-preview.tsx');
    await render(ContextPreviewDialog, props);
    assert.equal(reads.length, 0);
    assert.match(document.body.textContent, /does not show an earlier task/);
    await change(document.querySelector('select'), 'a');
    assert.equal(reads.length, 1);
    await change(document.querySelector('select'), 'b');
    assert.equal(reads[0].signal.aborted, true);
    assert.match(document.body.textContent, /SECOND_PRIVATE_CONTEXT/);
    await update(() => finishA(response(value('a', 'OLD_PRIVATE_CONTEXT'))));
    assert.ok(!document.body.textContent.includes('OLD_PRIVATE_CONTEXT'));
    assert.match(document.body.textContent, /SECOND_PRIVATE_CONTEXT/);
    assert.ok(reads.every(read => read.method === 'GET' && !read.body));
  }, options(async (path, request) => {
    reads.push(request);
    if (path.endsWith('chat=a')) return new Promise(resolve => { finishA = resolve; });
    return response(value('b', 'SECOND_PRIVATE_CONTEXT'));
  }));
});

test('context preview clears revoked or failed content, renders untrusted messages as text, and retains selection for retry', async () => {
  let denied = false;
  await reactFixture(async ({ render, change, click, button }) => {
    const { ContextPreviewDialog } = await import('../src/react/agents/context-preview.tsx');
    await render(ContextPreviewDialog, props);
    await change(document.querySelector('select'), 'a');
    assert.match(document.body.textContent, /onerror/);
    assert.equal(document.querySelectorAll('img').length, 0);
    assert.equal(button('Read full text').getAttribute('aria-expanded'), 'false');
    await click(button('Read full text'));
    assert.equal(button('Show less').getAttribute('aria-expanded'), 'true');
    denied = true;
    await click(button('Refresh preview'));
    assert.ok(!document.body.textContent.includes('onerror'));
    assert.match(document.querySelector('[role=alert]').textContent, /Access changed/);
    assert.equal(document.querySelector('select').value, 'a');
    denied = false;
    await click(button('Refresh preview'));
    assert.match(document.body.textContent, /onerror/);
  }, options(async () => denied ? response({ error: 'Access changed' }, 403)
    : response(value('a', '<img src=x onerror=alert(1)> ' + 'saved evidence '.repeat(80)))));
});

test('closing the context owner aborts pending reads and late content cannot reappear', async () => {
  let finish, signal;
  await reactFixture(async ({ render, change, update }) => {
    const { ContextPreviewDialog } = await import('../src/react/agents/context-preview.tsx');
    await render(ContextPreviewDialog, props);
    await change(document.querySelector('select'), 'a');
    await render(() => null);
    assert.equal(signal.aborted, true);
    await update(() => finish(response(value('a', 'LATE_PRIVATE_CONTEXT'))));
    assert.equal(document.querySelector('dialog'), null);
    assert.ok(!document.body.textContent.includes('LATE_PRIVATE_CONTEXT'));
  }, options(async (_path, request) => {
    signal = request.signal;
    return new Promise(resolve => { finish = resolve; });
  }));
});

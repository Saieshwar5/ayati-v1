import { test } from 'node:test';
import assert from 'node:assert/strict';
import { JSDOM } from 'jsdom';
import { applyDelta, mergeSnapshot } from '../src/conversation-state.ts';
import { renderConversation, renderFiles } from '../src/view.ts';

const snapshot = (content = '', state = 'streaming') => ({
  messages: [{ id: 'reply', task_id: 'task', role: 'assistant', content, state, created_at: '' }],
  tasks: [], artifacts: [],
});
const delta = (offset, text) => ({ kind: 'delta', task_id: 'task', message_id: 'reply', offset, text });

test('snapshot races and repeated deltas do not duplicate or remove streamed text', () => {
  let state = snapshot('Hello');
  assert.equal(applyDelta(state, delta(5, ' there')), true);
  state = mergeSnapshot(state, snapshot('Hello'));
  assert.equal(state.messages[0].content, 'Hello there');
  assert.equal(applyDelta(state, delta(5, ' there')), true);
  assert.equal(state.messages[0].content, 'Hello there');
  state = mergeSnapshot(state, snapshot('Hello there!'));
  applyDelta(state, delta(11, '!'));
  assert.equal(state.messages[0].content, 'Hello there!');
  state = mergeSnapshot(state, snapshot('Final authoritative text', 'complete'));
  assert.equal(state.messages[0].content, 'Final authoritative text');
});

test('stream gaps trigger resync and non-BMP characters use correct offsets', () => {
  const state = snapshot('Hi 🌱');
  assert.equal(applyDelta(state, delta(5, '!')), true);
  assert.equal(state.messages[0].content, 'Hi 🌱!');
  assert.equal(applyDelta(state, delta(20, 'gap')), false);
  assert.equal(state.messages[0].content, 'Hi 🌱!');
  assert.equal(applyDelta({ messages: [], tasks: [], artifacts: [] }, delta(5, 'missing')), false);
});

test('stored message and filename markup is rendered as text, never executable HTML', () => {
  const dom = new JSDOM('<div id="chat"></div><div id="files"></div>');
  globalThis.document = dom.window.document;
  const attack = '<img src=x onerror="alert(1)"><script>alert(2)</script>';
  renderConversation(document.querySelector('#chat'), snapshot(attack, 'complete'));
  assert.equal(document.querySelector('.message-content').textContent, attack);
  assert.equal(document.querySelector('img'), null);
  assert.equal(document.querySelector('script'), null);
  renderFiles(document.querySelector('#files'), [{ id: 'abc', name: attack, kind: 'output', size: 10 }]);
  assert.equal(document.querySelector('.file-name').textContent, attack);
  assert.equal(document.querySelector('a').getAttribute('href'), '/api/files/abc');
  assert.equal(document.querySelector('img'), null);
  dom.window.close();
});

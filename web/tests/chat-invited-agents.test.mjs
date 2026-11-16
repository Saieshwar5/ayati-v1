import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';

// Agents invited into a thread of your own agent's chat (schema 125).
const { requestAgents } = await import('../src/react/chat/mentions.ts');
const { threadSummary } = await import('../src/react/chat/thread-line.tsx');
const hr = '[@HR Desk](mention:agent:hr)', finance = '[@Finance Desk](mention:agent:finance)', legal = '[@Legal](mention:agent:legal)';

test('your own agent is asked; other agents may be mentioned only in its chat, at most two', () => {
  assert.deepEqual(requestAgents(`${hr} please fix this`, 'mine', true), ['mine']);
  assert.deepEqual(requestAgents(`${hr} ${finance} sort it out`, 'mine', true), ['mine']);
  assert.throws(() => requestAgents(`${hr} ${finance} ${legal}`, 'mine', true), /at most two agents/);
  assert.throws(() => requestAgents(`${hr} help`, 'shared'), /asks only its own agent/);
  assert.deepEqual(requestAgents('plain words', 'mine', true), ['mine']);
});

test('a thread with an invited agent says when it needs you and when it is done', () => {
  const root = { id: 'root', author: 'ravi', kind: 'user', content: `${hr} fix it`, state: 'sent', at: 1, thread: null };
  const reply = (id, author, extra = {}) => ({ id, author, kind: 'agent', content: 'x', state: 'complete', at: 2, thread: 'root', ...extra });
  const present = ['mine'];
  const working = [root, reply('a', 'mine', { state: 'queued', content: '' }), reply('b', 'hr')];
  assert.equal(threadSummary(working, 'root', present).state, undefined, 'still working');
  const asking = [root, reply('a', 'mine', { state: 'queued', progress: { state: 'input', text: '' } }), reply('b', 'hr')];
  assert.equal(threadSummary(asking, 'root', present).state, 'needs_you');
  const done = [root, reply('a', 'mine'), reply('b', 'hr')];
  assert.equal(threadSummary(done, 'root', present).state, 'done');
  // A thread with only the chat's own agents shows no state, as before.
  assert.equal(threadSummary([root, reply('a', 'mine')], 'root', present).state, undefined);
  assert.equal(threadSummary(done, 'root').state, undefined);
});

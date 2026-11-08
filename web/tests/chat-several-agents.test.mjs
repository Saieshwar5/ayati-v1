import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';
import { outboxStorage } from './outbox-storage-fixture.mjs';

// One message may ask up to three agents; each answers in its own reply.
const w = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${w}/?view=chat`;
const session = { user: { id: 'owner', email: 'owner@example.test' }, workspaces: [{ id: w, name: 'Store', role: 'Admin' }] };
const names = { atlas: 'Atlas', nova: 'Nova', orion: 'Orion', vega: 'Vega' };
const chat = { id: 'ops', name: 'ops', kind: 'channel', description: '', users: ['owner', 'p'], agents: Object.keys(names) };
const directory = { role: 'Admin', users: [{ id: 'owner', name: 'Ananya' }, { id: 'p', name: 'Priya' }], agents: Object.entries(names).map(([id, name]) => ({ id, name })) };
const mention = id => `[@${names[id]}](mention:agent:${id})`;
const reply = (id, agent, sequence, extra = {}) => ({ id, chat: 'ops', author: agent, kind: 'agent', content: '', state: 'queued', sequence, revision: 1, at: sequence, thread: null, attachments: [], ...extra });
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });

/** Opens the channel with a live stream, a device outbox and `post` answering each send. */
async function withChannel(post, run, draft) {
  let stream, stopLive;
  const original = { EventSource: globalThis.EventSource, IntersectionObserver: globalThis.IntersectionObserver, indexedDB: globalThis.indexedDB, sessionStorage: globalThis.sessionStorage };
  globalThis.EventSource = class { constructor() { this.listeners = new Map(); stream = this; } addEventListener(name, fn) { this.listeners.set(name, fn); } close() {} };
  globalThis.IntersectionObserver = class { observe() {} unobserve() {} disconnect() {} };
  globalThis.indexedDB = outboxStorage();
  try {
    await reactFixture(async fixture => {
      globalThis.sessionStorage = fixture.dom.window.sessionStorage;
      const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
      if (draft) sessionStorage.setItem(context.storageScope('relay.chat.draft.ops'), draft);
      const box = await import('../src/react/chat/outbox.ts');
      const { ConversationPane } = await import('../src/react/chat/conversation.tsx');
      const { startLive } = await import('../src/react/chat/live.ts');
      await fixture.render(ConversationPane, { chat, directory, back: () => {} });
      await fixture.update(() => { stopLive = startLive(); });
      let cursor = 1;
      const arrive = (...messages) => fixture.update(() => stream.listeners.get('changes')({ data: JSON.stringify({ cursor: cursor++, reset: false, events: messages.map(message => ({ kind: 'message', chat: 'ops', message })) }) }));
      const until = async condition => { for (let i = 0; i < 60 && !condition(); i++) await fixture.update(); assert.ok(condition()); };
      try { await run({ ...fixture, box, arrive, until, input: document.querySelector('.message-composer textarea') }); }
      finally { await fixture.update(() => { box.stopOutbox(); stopLive?.(); }); }
    }, { url, fetch: async (path, options = {}) => {
      const at = new URL(path, 'http://localhost').pathname;
      if (at.endsWith('/message-capabilities')) return response({ live: true });
      if (at.endsWith('/chats/ops/messages')) return options.method === 'POST' ? post(JSON.parse(options.body)) : response({ agents: Object.keys(names), next_before: null, messages: [] });
      return response(session);
    } });
  } finally { Object.assign(globalThis, original); }
}

test('a message to two agents sends both, retries the exact payload, and each agent replies on its own', async () => {
  const posts = []; let lost = true;
  await withChannel(body => {
    posts.push(body);
    if (lost) throw new TypeError('Connection dropped after commit');
    return response({ ok: true, message: { ...body, author: 'owner', kind: 'user', state: 'sent', at: 1, sequence: 1, chat: 'ops' } });
  }, async ({ input, change, click, submit, update, box, arrive, until }) => {
    for (const agent of ['atlas', 'nova']) {
      await change(input, `${input.value}@${names[agent].slice(0, 2)}`);
      await click(document.querySelector('[role=option]'));
    }
    await change(input, `${input.value}how many boxes are left?`);
    await submit(document.querySelector('form.message-composer'));
    await until(() => box.outboxSnapshot()[0]?.state === 'checking');
    assert.deepEqual(posts[0].agents, ['atlas', 'nova']);
    assert.equal(posts[0].agent, undefined, 'the older single agent field is not sent');
    assert.equal(posts[0].content, `${mention('atlas')} ${mention('nova')} how many boxes are left?`);
    // While the send is unconfirmed, the pending message says who it asks.
    const asking = () => [...document.querySelectorAll('[aria-label="Agents asked"] li')].map(li => li.textContent);
    assert.deepEqual(asking(), ['Asking Atlas…', 'Asking Nova…']);

    lost = false;
    await update(() => void box.retryMessage(box.outboxSnapshot()[0]));
    await until(() => box.outboxSnapshot().length === 0);
    assert.equal(posts.length, 2);
    assert.deepEqual(posts[1], posts[0], 'the retry sends exactly the same message and agents');

    // Each agent's reply is its own message, with its own waiting and typing state.
    await arrive({ ...posts[0], author: 'owner', kind: 'user', state: 'sent', at: 1, sequence: 1, chat: 'ops', revision: 1, thread: null },
      reply('r-atlas', 'atlas', 2), reply('r-nova', 'nova', 3));
    const replies = () => [...document.querySelectorAll('.chat-message:not(.chat-message-own)')].map(node => [
      node.getAttribute('aria-label'), node.querySelector('.chat-reply-progress')?.textContent ?? node.querySelector('.message-markdown, .inline-response')?.textContent ?? '']);
    assert.deepEqual(replies(), [['Message from Atlas', 'Waiting to start'], ['Message from Nova', 'Waiting to start']]);
    await arrive(reply('r-nova', 'nova', 3, { revision: 2, progress: { state: 'responding', text: '' } }));
    assert.deepEqual(replies().map(([, state]) => state), ['Waiting to start', 'Nova is typing…'], 'one agent working does not change the other');
    await arrive(reply('r-nova', 'nova', 3, { revision: 3, state: 'complete', content: 'Nova counted 12 boxes.' }));
    const [atlas, nova] = replies();
    assert.deepEqual(atlas, ['Message from Atlas', 'Waiting to start']);
    assert.equal(nova[0], 'Message from Nova');
    assert.match(document.querySelector('[data-message-id="r-nova"]').textContent, /Nova counted 12 boxes\./);
  });
});

test('a fourth agent is refused before sending, and the draft stays', async () => {
  const posts = [];
  const draft = `${['atlas', 'nova', 'orion', 'vega'].map(mention).join(' ')} check the count`;
  await withChannel(body => { posts.push(body); return response({ ok: true }); }, async ({ input, submit, box }) => {
    assert.equal(input.value, '@Atlas @Nova @Orion @Vega check the count');
    await submit(document.querySelector('form.message-composer'));
    assert.match(document.querySelector('.chat-error[role=alert]').textContent, /Ask at most 3 agents in one message\. Send another message for the others\./);
    assert.equal(posts.length, 0);
    assert.equal(box.outboxSnapshot().length, 0);
    assert.equal(input.value, '@Atlas @Nova @Orion @Vega check the count');
  }, draft);
});

test('an emoji-only message to two agents shows their reactions instead of replies that wait or type', async () => {
  const posts = [];
  // The server stores the message and each agent reacts to it in the same step;
  // no reply is created, so nothing waits or types (D246).
  await withChannel(body => {
    posts.push(body);
    return response({ ok: true, message: { ...body, author: 'owner', kind: 'user', state: 'sent', at: 1, sequence: 1, chat: 'ops', revision: 1, thread: null,
      delivered: 2, seen: 2, reactions: [], agent_reactions: body.agents.map(agent => ({ agent, emoji: '👍', name: names[agent] })) } });
  }, async ({ input, change, click, submit, box, until }) => {
    for (const agent of ['atlas', 'nova']) {
      await change(input, `${input.value}@${names[agent].slice(0, 2)}`);
      await click(document.querySelector('[role=option]'));
    }
    await change(input, `${input.value}👍`);
    await submit(document.querySelector('form.message-composer'));
    await until(() => box.outboxSnapshot().length === 0);
    assert.equal(posts.length, 1);
    assert.equal(posts[0].content, `${mention('atlas')} ${mention('nova')} 👍`);
    const chips = () => [...document.querySelectorAll('.chat-message-own .chat-agent-reaction')].map(chip => chip.textContent);
    await until(() => chips().length === 2);
    assert.deepEqual(chips(), ['Atlas reacted 👍', 'Nova reacted 👍']);
    assert.equal(document.querySelectorAll('.chat-reply-progress, .chat-typing').length, 0, 'no reply waits or types');
    assert.doesNotMatch(document.querySelector('#root').textContent, /is typing|Waiting to start/);
  });
});

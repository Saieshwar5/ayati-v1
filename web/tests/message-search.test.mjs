import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { reactFixture } from './react-fixture.mjs';

const workspace = '11111111-1111-4111-8111-111111111111';
const url = `http://localhost/w/${workspace}/?view=chat`;
const session = { user: { id: 'owner', email: 'owner@example.test' }, workspaces: [{ id: workspace, name: 'Store', role: 'Admin' }] };
const directory = { role: 'Admin', users: [{ id: 'owner', name: 'Owner' }, { id: 'p', name: 'Priya' }], agents: [{ id: 'a', name: 'Stock assistant' }] };
const chats = [{ id: 'ops', name: 'ops', kind: 'channel', description: '', users: ['owner', 'p'], agents: [] },
  { id: 'dm', name: 'Priya', kind: 'direct', description: '', users: ['owner', 'p'], agents: [] }];
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const hit = (id, chat, snippet, extra = {}) => ({ id, chat, chat_kind: chat === 'dm' ? 'direct' : 'channel', chat_name: chat, author: 'p', kind: 'user',
  name: 'Priya', at: 1793962080, thread: null, has_files: false, snippet, ...extra });
const wait = ms => new Promise(resolve => setTimeout(resolve, ms));

async function load(module, name) {
  const context = await import('../src/workspace-context.ts');
  await context.enterWorkspace();
  return (await import(module))[name];
}

test('the search dialog finds messages after a pause in typing and opens the one chosen', async () => {
  const asked = [];
  await reactFixture(async ({ render, button, click, change, update, navigation }) => {
    const Search = await load('../src/react/chat/search-conversations.tsx', 'SearchConversations');
    const opened = [];
    await render(() => createElement(Search, { directory, chats, close: () => {}, opened: () => {}, openMessage: (chat, message) => opened.push([chat, message]) }));
    const input = document.querySelector('input');
    await change(input, 'd');
    assert.match(document.body.textContent, /No matching conversations/);
    assert.equal(document.querySelector('.chat-message-search'), null, 'one letter searches no messages');
    await change(input, 'de'); await change(input, 'deli'); await change(input, 'deliv');
    await wait(400); await update();
    assert.deepEqual(asked, ['q=deliv'], 'one request after typing pauses');
    const results = [...document.querySelectorAll('.chat-message-hits button')];
    assert.equal(results.length, 2);
    assert.match(results[0].textContent, /Priya.*#ops/);
    assert.equal(results[0].querySelector('mark').textContent, 'delivery');
    assert.match(results[1].textContent, /Priya.*in a thread.*has files/);
    await click(button('Messages'));
    assert.equal(document.querySelector('.chat-search-results'), null, 'Messages shows only messages');
    await click(document.querySelector('.chat-message-hits button'));
    assert.deepEqual(opened, [['ops', 'm1']]);
    assert.deepEqual(navigation, []);
  }, { url, fetch: async (path, options = {}) => {
    if (path.includes('/message-search')) {
      asked.push(path.split('?')[1]);
      return response({ terms: ['deliv'], next_before: null, results: [
        hit('m1', 'ops', 'The delivery date moved'),
        hit('m2', 'dm', 'Delivered the files', { thread: 'root', has_files: true }) ] });
    }
    return response(session);
  } });
});

test('a slow answer for an older search never replaces the current one', async () => {
  const pending = [];
  await reactFixture(async ({ render, change, update }) => {
    const Search = await load('../src/react/chat/search-conversations.tsx', 'SearchConversations');
    await render(() => createElement(Search, { directory, chats, close: () => {}, opened: () => {}, openMessage: () => {} }));
    const input = document.querySelector('input');
    await change(input, 'invoice');
    await wait(400); await update();
    await change(input, 'refund');
    await wait(400); await update();
    assert.equal(pending.length, 2);
    assert.equal(pending[0].signal.aborted, true, 'the older request is cancelled');
    await update(() => { pending[1].resolve(response({ terms: ['refund'], next_before: null, results: [hit('new', 'ops', 'refund approved')] })); });
    await update(() => { pending[0].resolve(response({ terms: ['invoice'], next_before: null, results: [hit('old', 'ops', 'invoice sent')] })); });
    const texts = [...document.querySelectorAll('.chat-hit-text')].map(node => node.textContent);
    assert.deepEqual(texts, ['refund approved']);
  }, { url, fetch: (path, options = {}) => {
    if (!path.includes('/message-search')) return Promise.resolve(response(session));
    return new Promise(resolve => pending.push({ resolve, signal: options.signal }));
  } });
});

test('searching inside a conversation asks for that conversation only and opens the message there', async () => {
  const asked = [];
  await reactFixture(async ({ dom, render, click, change, update, navigation }) => {
    const ConversationSearch = await load('../src/react/chat/conversation-search.tsx', 'ConversationSearch');
    const closed = [];
    await render(() => createElement(ConversationSearch, { chat: chats[0], title: 'ops', directory, close: () => closed.push(true) }));
    assert.match(document.querySelector('dialog').textContent, /Search in ops/);
    await change(document.querySelector('input'), 'pricing');
    await wait(400); await update();
    assert.deepEqual(asked, ['q=pricing&chat=ops']);
    assert.doesNotMatch(document.querySelector('.chat-hit-meta').textContent, /#ops/, 'the conversation name is not repeated');
    await click(document.querySelector('.chat-message-hits button'));
    assert.deepEqual(closed, [true]);
    assert.deepEqual(navigation, [], 'stays on the page');
    assert.equal(dom.window.location.search, '?view=chat&conversation=ops&message=m9');
  }, { url, fetch: async (path) => {
    if (path.includes('/message-search')) { asked.push(path.split('?')[1]); return response({ terms: ['pricing'], next_before: null, results: [hit('m9', 'ops', 'pricing for Mehta')] }); }
    return response(session);
  } });
});

test('older results load on demand and a failed search says why', async () => {
  let calls = 0;
  await reactFixture(async ({ render, button, click, change, update }) => {
    const Search = await load('../src/react/chat/search-conversations.tsx', 'SearchConversations');
    await render(() => createElement(Search, { directory, chats, close: () => {}, opened: () => {}, openMessage: () => {} }));
    await click(button('Messages'));
    await change(document.querySelector('input'), 'standup');
    await wait(400); await update();
    assert.match(document.body.textContent, /1\+ message/);
    await click(button('Older messages'));
    assert.equal(document.querySelectorAll('.chat-message-hits li').length, 2);
    assert.equal(button('Older messages'), undefined);
    await change(document.querySelector('input'), 'broken');
    await wait(400); await update();
    assert.match(document.querySelector('[role=alert]').textContent, /Search is unavailable/);
  }, { url, fetch: async (path) => {
    if (!path.includes('/message-search')) return response(session);
    calls++;
    if (path.includes('broken')) return response({ error: 'Search is unavailable' }, 503);
    return path.includes('before=')
      ? response({ terms: ['standup'], next_before: null, results: [hit('m2', 'ops', 'standup two')] })
      : response({ terms: ['standup'], next_before: 41, results: [hit('m1', 'ops', 'standup one')] });
  } });
});

test('filter chips and written in:, from: and has: narrow the search, and each written filter has a chip to remove it', async () => {
  const asked = [];
  await reactFixture(async ({ render, click, change, update }) => {
    const Search = await load('../src/react/chat/search-conversations.tsx', 'SearchConversations');
    await render(() => createElement(Search, { directory, chats, close: () => {}, opened: () => {}, openMessage: () => {} }));
    const input = document.querySelector('input');
    const chip = label => [...document.querySelectorAll('.chat-search-chips button')].find(b => b.textContent === label);
    const last = async () => { await wait(400); await update(); return new URLSearchParams(asked.at(-1)); };

    await change(input, 'in:#ops from:priya has:link supplier');
    let params = await last();
    assert.deepEqual(Object.fromEntries(params), { q: 'supplier', chat: 'ops', from: 'p', links: 'true' });
    const written = [...document.querySelectorAll('.chat-search-chip.written')].map(c => c.firstChild.textContent);
    assert.deepEqual(written, ['in: #ops', 'from: Priya', 'has: link']);

    // A chip's × takes its filter out of what was typed.
    await click(document.querySelector('button[aria-label="Remove in: #ops"]'));
    assert.equal(input.value, 'from:priya has:link supplier');
    params = await last();
    assert.equal(params.get('chat'), null);

    // Toggle chips: From me replaces a written from:, the others add their filter.
    await click(chip('From me'));
    await click(chip('Mentions me'));
    await click(chip('From an agent'));
    await click(chip('Has a file'));
    await click(chip('In threads'));
    params = await last();
    assert.deepEqual(Object.fromEntries(params), { q: 'supplier', from: 'owner', files: 'true', links: 'true', mentions_me: 'true', agents: 'true', threads: 'true' });
    assert.equal(chip('From me').getAttribute('aria-pressed'), 'true');

    // A written filter that names nothing says so, and the search goes on without it.
    await change(input, 'in:#nowhere supplier');
    await last();
    assert.match(document.querySelector('.chat-message-search [role=alert]').textContent, /Nothing matches in:#nowhere; searching without it\./);
  }, { url, fetch: async path => {
    if (path.includes('/message-search')) { asked.push(path.split('?')[1]); return response({ results: [], terms: ['supplier'], next_before: null }); }
    return response(path.endsWith('/session') ? session : {});
  } });
});

test('opening a result remembers the search for this person; recent searches are one tap away and can be cleared', async () => {
  await reactFixture(async ({ render, click, change, update, button }) => {
    const Search = await load('../src/react/chat/search-conversations.tsx', 'SearchConversations');
    const props = { directory, chats, close: () => {}, opened: () => {}, openMessage: () => {} };
    await render(() => createElement(Search, props));
    await change(document.querySelector('input'), 'invoice 2041');
    await wait(400); await update();
    await click(document.querySelector('.chat-message-hits button'));
    assert.deepEqual(JSON.parse(localStorage.getItem(`relay.chat.recent-searches.owner.${workspace}`)), ['invoice 2041']);

    await render(() => createElement('p', null, 'closed'));
    await render(() => createElement(Search, props));
    const recent = document.querySelector('[aria-label="Recent searches"]');
    assert.match(recent.textContent, /Recent searches.*invoice 2041.*Clear/);
    await click([...recent.querySelectorAll('button')].find(b => b.textContent === 'invoice 2041'));
    assert.equal(document.querySelector('input').value, 'invoice 2041');
    assert.equal(document.querySelector('[aria-label="Recent searches"]'), null, 'hidden once something is typed');
    await change(document.querySelector('input'), '');
    await click(button('Clear'));
    assert.equal(document.querySelector('[aria-label="Recent searches"]'), null);
    assert.equal(localStorage.getItem(`relay.chat.recent-searches.owner.${workspace}`), null);
  }, { url, fetch: async path => {
    if (path.includes('/message-search')) return response({ results: [hit('m9', 'ops', 'invoice 2041 paid')], terms: ['invoice', '2041'], next_before: null });
    return response(path.endsWith('/session') ? session : {});
  } });
});

test('written filters resolve names, first names and "me"', async () => {
  const context = await import('../src/workspace-context.ts'); void context;
  const { parseSearch, withoutFilter } = await import('../src/react/chat/search-query.ts');
  const parsed = parseSearch('from:me in:pri has:files hello  there', { chats, directory, me: 'owner' });
  assert.equal(parsed.words, 'hello there');
  assert.deepEqual([parsed.from, parsed.chat?.id, parsed.files], [{ id: 'owner', label: 'me' }, 'dm', true]);
  assert.equal(parseSearch('from:stock x', { chats, directory }).from.id, 'a', 'agents too, by any word of the name');
  assert.deepEqual(parseSearch('has:pdf x', { chats, directory }).unknown, ['has:pdf']);
  assert.equal(withoutFilter('has:file has:link x', 'has', 'link'), 'has:file x');
});

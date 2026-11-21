import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement, useState } from 'react';
import { reactFixture } from './react-fixture.mjs';
import { outboxStorage } from './outbox-storage-fixture.mjs';

// Asking your own assistant from a message box (2026-11-21): mention it, say what you want,
// and it writes a reply only you see. What you asked is never posted; the reply is sent by
// you, as you. The assistant here is named Tara: nothing depends on its name. What the box
// asks and the @ list are in assistant-ask-model.test.mjs.
const w = '11111111-1111-4111-8111-111111111111', url = `http://localhost/w/${w}/?view=chat&conversation=sales`;
const session = { user: { id: 'ravi', email: 'ravi@example.test' }, workspaces: [{ id: w, name: 'Northwind', role: 'Member' }] };
const tara = { id: 'tara', name: 'Tara' };
const mention = '[@Tara](mention:agent:tara)';
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const said = (id, author, content, sequence) => ({ id, author, name: author, kind: 'user', content, state: 'sent', at: sequence, sequence });
const karthik = said('m1', 'karthik', 'How are November sales going against target?', 1);

/** Every write the page makes; drafts are numbered in the order they were asked for. */
function server(writes, { text = 'November is at 86% of target.', fail = 0 } = {}) {
  let drafts = 0;
  return async (path, options = {}) => {
    const short = path.replace(`/api/identity/workspaces/${w}`, '');
    if (path.endsWith('/session')) return response(session);
    if (options.method !== 'POST') return short.startsWith('/assistant/drafts/') ? response({ state: 'complete', text }) : response({ threads: [] });
    const body = JSON.parse(options.body);
    writes.push({ path: short, body });
    if (writes.length <= fail) return response({ error: 'Your assistant is paused' }, 400);
    if (short === '/assistant/drafts') return response({ ok: true, reply: `r${++drafts}` });
    if (short.endsWith('/messages')) return response({ ok: true, message: { ...body, author: 'ravi', kind: 'user', state: 'sent', at: 9, chat: 'sales' } });
    return response({ ok: true });
  };
}
const button = label => [...document.querySelectorAll('button')].find(node => node.textContent.trim() === label);
const until = async fn => { for (let i = 0; i < 50 && !fn(); i++) await new Promise(resolve => setImmediate(resolve)); assert.ok(fn()); };

/** A message box with the hook, as a conversation uses it. `page.box` is its text. */
async function harness({ render, update, dom }, page) {
  let tick; dom.window.setInterval = fn => { tick = fn; return 1; }; dom.window.clearInterval = () => {};
  globalThis.sessionStorage = dom.window.sessionStorage;
  await (await import('../src/workspace-context.ts')).enterWorkspace();
  const model = await import('../src/react/chat/assistant-threads-model.ts');
  const outbox = await import('../src/react/chat/outbox.ts');
  const { useAssistantAsk } = await import('../src/react/chat/assistant-ask.tsx');
  function Box({ chat, messages, quote }) {
    const [value, setValue] = useState(page.box);
    const change = next => { page.box = next; setValue(next); };
    const post = async (content, quoted) => {
      const id = crypto.randomUUID();
      await outbox.enqueue(chat, { id, content, reply_to: quoted, attachments: [] });
      return id;
    };
    const assist = useAssistantAsk({ chat, where: 'in #sales', direct: false, assistant: tara, viewer: 'ravi', value, change, messages, quote, post, sent: {} });
    page.assist = assist;
    return createElement('div', null, assist.card, assist.notice, assist.tool,
      createElement('button', { onClick: () => void assist.ask() }, 'Submit'));
  }
  const show = (props = {}) => render(() => createElement(model.AssistantThreads.Provider, { value: { threads: [], reload() {}, assistant: 'tara', room: 'room' } },
    createElement(Box, { chat: 'sales', messages: [karthik], ...props })));
  return { show, outbox, tick: () => update(() => { void tick(); }) };
}
const fixture = (run, writes, options) => {
  const previous = globalThis.indexedDB; globalThis.indexedDB = outboxStorage();
  return reactFixture(async tools => {
    const page = { box: '' }, kit = await harness(tools, page);
    try { await run({ ...tools, ...kit, page }); } finally { kit.outbox.stopOutbox(); }
  }, { url, fetch: server(writes, options) }).finally(() => { globalThis.indexedDB = previous; delete globalThis.sessionStorage; });
};

test('asking posts nothing in the conversation; the draft is sent by you, once, and recorded as your assistant’s', async () => {
  const writes = [];
  await fixture(async ({ show, tick, update, page }) => {
    await show();
    assert.equal(document.querySelector('.assistant-ask'), null);
    // The button beside Attach starts a request: it mentions your assistant and sends nothing.
    await update(() => document.querySelector('button[aria-label="Ask Tara"]').click());
    assert.equal(page.box, `${mention} `);
    assert.equal(writes.length, 0);
    page.box = `${mention} go through November sales and answer [@Karthik](mention:user:karthik)`;
    await show();
    assert.match(document.querySelector('.composer-asking').textContent, /Asking Tara.*Only you see this\. Nothing is sent in #sales\./);
    await update(() => { button('Submit').click(); button('Submit').click(); });
    assert.equal(writes.length, 1, 'asked once despite a double submit');
    assert.equal(writes[0].path, '/assistant/drafts', 'the request goes to your assistant, not to the conversation');
    assert.deepEqual({ ...writes[0].body, id: undefined }, { id: undefined, chat: 'sales', thread: null, instruction: 'go through November sales and answer @Karthik', message: null, previous: null });
    assert.equal(page.box, '', 'the message box is free while it writes');
    assert.match(document.querySelector('.assistant-ask').textContent, /Tara is writing a reply for you.*Only you see this/);
    await tick();
    const card = document.querySelector('.assistant-ask');
    assert.match(card.getAttribute('aria-label'), /Draft from Tara, only you see this/);
    assert.match(card.textContent, /Will be sent as you in #sales/);
    assert.match(card.textContent, /You asked: go through November sales and answer @Karthik/);
    assert.equal(card.querySelector('.assistant-ask-text').textContent, 'November is at 86% of target.');
    assert.equal(writes.length, 1, 'nothing is sent by writing it');
    await update(() => { button('Send as me').click(); button('Send as me').click(); });
    await until(() => writes.some(write => write.path.endsWith('/used')));
    const posted = writes.filter(write => write.path === '/chats/sales/messages');
    assert.equal(posted.length, 1, 'sent once despite a double click');
    assert.equal(posted[0].body.content, 'November is at 86% of target.', 'the reply, not what you asked');
    assert.deepEqual(writes.at(-1), { path: '/assistant/drafts/r1/used', body: { message: posted[0].body.id } });
    await update();
    assert.equal(document.querySelector('.assistant-ask'), null);
  }, writes);
});

test('“respond” answers the newest message meant for you; a moved-on conversation and a change ask again', async () => {
  const writes = [];
  await fixture(async ({ show, tick, update, change, page }) => {
    page.box = `${mention} respond`;
    await show();
    await update(() => button('Submit').click());
    assert.deepEqual([writes[0].body.instruction, writes[0].body.message], ['', 'm1']);
    await tick();
    assert.match(document.querySelector('.assistant-ask-quote').textContent, /Answering karthik: “How are November sales going against target\?”/);
    assert.match(button('Send as me').className, /primary/);
    // Someone writes while the draft waits: it may no longer fit, so sending is no longer the main action.
    await show({ messages: [karthik, said('m2', 'karthik', 'Only wholesale, please.', 2), said('m3', 'ravi', 'One moment.', 3)] });
    assert.match(document.querySelector('.assistant-ask-moved').textContent, /1 new message since Tara wrote this\./);
    assert.doesNotMatch(button('Send as me').className, /primary/);
    await update(() => button('Update draft').click());
    assert.deepEqual([writes[1].body.instruction, writes[1].body.message, writes[1].body.previous], ['', 'm1', null]);
    await tick();
    assert.equal(document.querySelector('.assistant-ask-moved'), null, 'read again as the conversation is now');
    // A change is asked of this draft.
    await change(document.querySelector('.assistant-ask-change input'), 'shorter');
    await update(() => document.querySelector('.assistant-ask-change button').click());
    assert.deepEqual([writes[2].body.instruction, writes[2].body.previous], ['shorter', 'r2']);
    await tick();
    assert.match(document.querySelector('.assistant-ask').textContent, /Draft from Tara/);
    assert.equal(writes.filter(write => write.path.endsWith('/messages')).length, 0);
  }, writes);
});

test('a refused request keeps your words; Edit moves the draft into the box and Discard gives the box back', async () => {
  const writes = [];
  await fixture(async ({ show, tick, update, page }) => {
    page.box = `${mention} say Friday works`;
    await show();
    await update(() => button('Submit').click());
    assert.equal(page.box, `${mention} say Friday works`, 'kept for another try');
    assert.match(document.querySelector('[role=alert]').textContent, /Your assistant is paused/);
    await update(() => button('Submit').click());
    await tick();
    page.box = 'half a thought'; await show();
    await update(() => button('Edit').click());
    assert.equal(page.box, 'November is at 86% of target.');
    assert.equal(document.querySelector('.assistant-ask'), null);
    assert.match(document.querySelector('.composer-notice').textContent, /Draft from Tara.*Edit it, then send/);
    await update(() => button('Discard').click());
    assert.equal(page.box, 'half a thought', 'what you had typed is back');
    assert.equal(document.querySelector('.composer-notice'), null);
  }, writes, { fail: 1 });
});

test('a typed name warns before it is posted, a draft survives leaving the conversation, and your assistant’s own chat has none of it', async () => {
  const writes = [];
  await fixture(async ({ show, tick, update, page, render }) => {
    page.box = '@tara tell him wholesale is behind, not why';
    await show();
    assert.equal(page.assist.asking, false, 'as written this is an ordinary message');
    assert.match(document.querySelector('.composer-warning').textContent, /This would be sent in #sales as written\..*Did you mean to ask Tara\?/);
    await update(() => button('Ask Tara instead').click());
    assert.equal(page.box, `${mention} tell him wholesale is behind, not why`);
    assert.equal(page.assist.asking, true);
    await update(() => button('Submit').click());
    await tick();
    // Leaving and coming back: the draft is still here, for this person in this workspace.
    await render(() => createElement('p', null, 'Another page'));
    await show();
    assert.equal(document.querySelector('.assistant-ask-text').textContent, 'November is at 86% of target.');
    assert.ok(Object.keys(sessionStorage).every(key => !key.startsWith('relay.chat.ask.') || key.endsWith(`.ravi.${w}`)));
    await update(() => button('Discard').click());
    await render(() => createElement('p', null, 'Another page'));
    await show();
    assert.equal(document.querySelector('.assistant-ask'), null, 'a discarded draft does not come back');
    await show({ chat: 'room' });
    assert.equal(document.querySelector('button[aria-label="Ask Tara"]'), null);
    assert.equal(page.assist.asking, false);
  }, writes);
});

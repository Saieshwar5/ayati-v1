import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement, Fragment } from 'react';
import { reactFixture } from './react-fixture.mjs';

// Forwarding a message (schema 125): pick a person or agent, say what you need,
// send once, and land in your chat with them. A forward shows its copy. The
// message's files go along unless left out, and your own documents are uploaded
// when you press Forward.
const w = '11111111-1111-4111-8111-111111111111', url = `http://localhost/w/${w}/?view=chat&conversation=dm-priya`;
const session = { user: { id: 'ravi', email: 'ravi@example.test' }, workspaces: [{ id: w, name: 'Northwind', role: 'Member' }] };
const directory = { role: 'Member',
  users: [{ id: 'ravi', name: 'Ravi Kumar', title: '', photo: null }, { id: 'priya', name: 'Priya Sharma', title: 'Head of Operations', photo: null },
    { id: 'arjun', name: 'Arjun Patel', title: 'Finance', photo: null }],
  agents: [{ id: 'finance', name: 'Finance Desk', role: 'Invoices and payments', audience: 'everyone' },
    { id: 'mine', name: 'My assistant', role: 'Personal assistant', audience: 'only_me', yours: true },
    { id: 'theirs', name: 'Priya’s assistant', role: 'Personal assistant', audience: 'only_me' },
    { id: 'orders', name: 'Orders Desk', role: 'Orders and deliveries', audience: 'everyone', paused: true }] };
// Ravi last talked to Priya, where the message is, and before that to Arjun.
const chats = [{ id: 'dm-priya', name: 'Direct', kind: 'direct', description: '', users: ['ravi', 'priya'], agents: [], activity: 50 },
  { id: 'dm-arjun', name: 'Direct', kind: 'direct', description: '', users: ['ravi', 'arjun'], agents: [], activity: 40 }];
const pdf = (id, name) => ({ id, name, mime: 'application/octet-stream', size: 2048, checksum: 'c' });
const asked = { id: 'p1', chat: 'dm-priya', author: 'priya', name: 'Priya Sharma', kind: 'user', content: 'Acme says invoice INV-2291 is unpaid.', state: 'sent', at: 1763283720 };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
function server(writes, { fail = 0, hold } = {}) {
  let forwards = 0;
  return async (path, options = {}) => {
    if (options.method === 'POST') {
      const short = path.replace(`/api/identity/workspaces/${w}`, '');
      const upload = short.includes('/objects');
      writes.push({ path: short, body: upload ? options.body.name : JSON.parse(options.body) });
      if (upload) return response(pdf(`stored-${options.body.name}`, options.body.name));
      if (short === '/chats') return response({ id: 'dm-finance' });
      if (++forwards <= fail) return response({ error: 'The agent is busy' }, 409);
      return hold ? new Promise(resolve => { hold.release = () => resolve(response({ ok: true, chat: 'dm-finance' })); }) : response({ ok: true, chat: 'dm-finance' });
    }
    if (path.endsWith('/session')) return response(session);
    if (path.endsWith('/chats')) return response({ chats });
    return response({});
  };
}
async function show(render, list = [asked]) {
  globalThis.IntersectionObserver = class { observe() {} unobserve() {} disconnect() {} };
  await (await import('../src/workspace-context.ts')).enterWorkspace();
  const { ChatMessage } = await import('../src/react/chat/message.tsx');
  await render(() => createElement(Fragment, null, list.map(message => createElement(ChatMessage, { key: message.id, message, directory, place: 'Direct message', openThread() {} }))));
}
const article = id => document.querySelector(`[data-message-id="${id}"]`);
const dialog = () => [...document.querySelectorAll('dialog')].find(node => node.textContent.includes('Forward message'));
const button = (node, label) => [...node.querySelectorAll('button')].find(b => b.textContent.trim() === label || b.getAttribute('aria-label') === label);
const rows = () => [...dialog().querySelectorAll('.chat-forward-list button')];
const choose = async (click, name) => click(rows().find(b => b.textContent.includes(name)));
// Someone who is not offered is found by typing, as Priya is: the message is already in her conversation.
const search = async (change, click, name) => { await change(dialog().querySelector('input[role=combobox]'), name); await choose(click, name); };
const key = (update, node, name) => update(() => node.dispatchEvent(new window.KeyboardEvent('keydown', { key: name, bubbles: true, cancelable: true })));
const forwards = writes => writes.filter(write => write.path.endsWith('/forward'));

test('forwarding to an agent needs your words, sends once and opens your chat with it', async () => {
  const writes = [], hold = {};
  await reactFixture(async ({ render, click, change, update }) => {
    await show(render);
    await click(button(article('p1'), 'Forward'));
    assert.ok(dialog(), 'the forward window opens');
    assert.equal(document.activeElement.getAttribute('aria-label'), 'Forward to', 'typing searches at once');
    const names = rows().map(b => b.textContent);
    assert.match(names[0], /Arjun Patel/, 'the most recent other conversation comes first, not the one the message is in');
    assert.match(dialog().querySelector('.chat-forward-list').textContent, /^Recent/);
    assert.ok(names.some(name => name.includes('Finance Desk')) && names.some(name => name.includes('My assistant')));
    assert.equal(names.some(name => name.includes('Priya’s assistant')), false, 'not someone else’s only-me agent');
    assert.equal(names.some(name => name.includes('Ravi Kumar')), false, 'not yourself');
    assert.match(dialog().querySelector('.chat-forwarded').textContent, /Priya Sharma.*Direct message/);
    await choose(click, 'Finance Desk');
    assert.match(dialog().textContent, /owners and admins can see what you forward/);
    assert.equal(rows().length, 0, 'the list closes once someone is chosen');
    assert.equal(document.activeElement, dialog().querySelector('textarea'), 'ready for your words');
    assert.equal(button(dialog(), 'Forward to Finance Desk').disabled, true, 'an agent needs to know what you need');
    await change(dialog().querySelector('textarea'), 'Find out why this is unpaid.');
    const send = button(dialog(), 'Forward to Finance Desk');
    assert.equal(send.disabled, false);
    await update(() => { send.click(); send.click(); });
    assert.equal(writes.length, 1, 'one request despite a double click');
    assert.equal(writes[0].path, '/chats/dm-priya/messages/p1/forward');
    assert.deepEqual({ ...writes[0].body, id: undefined, chat: undefined }, { id: undefined, chat: undefined, to: { kind: 'agent', id: 'finance' }, note: 'Find out why this is unpaid.', files: [], attachments: [] });
    assert.match(writes[0].body.id, /^[0-9a-f-]{36}$/);
    await update(() => hold.release());
    assert.equal(dialog(), undefined, 'the window closes once sent');
    assert.match(window.location.href, /conversation=dm-finance/, 'it opens your chat with the agent');
  }, { url, fetch: server(writes, { hold }) });
});

test('a failed forward keeps the window and your words, and a retry reuses its ID', async () => {
  const writes = [];
  await reactFixture(async ({ render, click, change }) => {
    await show(render);
    await click(button(article('p1'), 'Forward'));
    await search(change, click, 'Priya Sharma');
    assert.equal(button(dialog(), 'Forward to Priya Sharma').disabled, false, 'a person needs no words of yours');
    await change(dialog().querySelector('textarea'), 'FYI');
    await click(button(dialog(), 'Forward to Priya Sharma'));
    assert.ok(dialog(), 'still open after the failure');
    assert.match(dialog().textContent, /busy/);
    assert.equal(dialog().querySelector('textarea').value, 'FYI');
    await click(button(dialog(), 'Forward to Priya Sharma'));
    assert.equal(writes.length, 2);
    assert.equal(writes[1].body.id, writes[0].body.id, 'the same forward keeps its ID');
    assert.deepEqual(writes[1].body.to, { kind: 'user', id: 'priya' });
  }, { url, fetch: server(writes, { fail: 1 }) });
});

test('a forward shows its copy, and a bare forward shows only the copy', async () => {
  const copy = { name: 'Finance Desk', kind: 'agent', place: '#finance', at: 1763283720, text: 'Paid on **20 November**.' };
  const list = [
    { id: 'f1', chat: 'dm-priya', author: 'ravi', name: 'Ravi Kumar', kind: 'user', content: 'Found it.', state: 'sent', at: 2, forwarded: copy },
    { id: 'f2', chat: 'dm-priya', author: 'priya', name: 'Priya Sharma', kind: 'user', content: 'Forwarded message', state: 'sent', at: 3, forwarded: { ...copy, kind: 'user', name: 'Arjun Patel' } }];
  await reactFixture(async ({ render }) => {
    await show(render, list);
    const first = article('f1').querySelector('.chat-forwarded');
    assert.match(first.textContent, /Forwarded from Finance Desk.*AI.*#finance/);
    assert.equal(first.querySelector('strong + span, .ai-tag')?.textContent, 'AI');
    assert.equal(first.querySelector('.message-markdown strong').textContent, '20 November');
    assert.match(article('f1').textContent, /Found it\./);
    assert.match(article('f2').textContent, /Forwarded from Arjun Patel/);
    assert.equal(article('f2').textContent.includes('Forwarded message'), false, 'no placeholder text');
  }, { url, fetch: server([]) });
});

test('the keyboard searches and chooses, and a paused agent cannot be chosen', async () => {
  await reactFixture(async ({ render, click, change, update }) => {
    await show(render);
    await click(button(article('p1'), 'Forward'));
    const field = () => dialog().querySelector('input[role=combobox]');
    await change(field(), 'desk');
    assert.deepEqual(rows().map(b => b.querySelector('.chat-forward-row-name').textContent), ['Finance Desk', 'Orders Desk']);
    await key(update, field(), 'ArrowDown');
    assert.equal(rows()[1].getAttribute('aria-selected'), 'true');
    assert.match(rows()[1].textContent, /Paused/);
    await key(update, field(), 'Enter');
    assert.ok(field(), 'a paused agent takes no new messages');
    await click(rows()[1]);
    assert.ok(field(), 'nor by clicking it');
    await key(update, field(), 'ArrowUp');
    await key(update, field(), 'Enter');
    assert.equal(field(), null);
    assert.match(dialog().querySelector('.chat-forward-chip').textContent, /Finance Desk/);
    assert.equal(document.activeElement, dialog().querySelector('textarea'));
    await click(button(dialog(), 'Choose someone other than Finance Desk'));
    assert.equal(document.activeElement, field(), 'back in the search, empty');
    assert.equal(field().value, '');
    await change(field(), 'nobody');
    assert.match(dialog().textContent, /No one matches “nobody”/);
  }, { url, fetch: server([]) });
});

test('the files you keep and your own documents go along, and a retry stores nothing twice', async () => {
  const writes = [];
  const sent = { ...asked, id: 'p2', content: 'Here is the invoice and the statement.', attachments: [pdf('inv', 'invoice.pdf'), pdf('stm', 'statement.pdf')] };
  await reactFixture(async ({ render, click, change, update }) => {
    await show(render, [sent]);
    await click(button(article('p2'), 'Forward'));
    await choose(click, 'Finance Desk');
    assert.match(dialog().querySelector('.chat-forwarded').textContent, /invoice\.pdf.*statement\.pdf/);
    await click(button(dialog(), 'Leave out statement.pdf'));
    assert.ok(button(dialog(), 'Include statement.pdf'), 'still listed, and can be put back');
    const picker = dialog().querySelector('input[type=file]');
    const mine = new window.File(['policy'], 'policy.pdf', { type: 'application/pdf' });
    Object.defineProperty(picker, 'files', { value: [mine], configurable: true });
    await update(() => picker.dispatchEvent(new window.Event('change', { bubbles: true })));
    assert.ok(button(dialog(), 'Remove policy.pdf'));
    assert.equal(writes.length, 0, 'nothing is stored before you press Forward');
    await change(dialog().querySelector('textarea'), 'Is this paid?');
    await click(button(dialog(), 'Forward to Finance Desk'));
    assert.match(dialog().textContent, /busy/, 'the forward failed after the upload');
    assert.ok(button(dialog(), 'Remove policy.pdf'), 'your document is kept');
    await click(button(dialog(), 'Forward to Finance Desk'));
    assert.deepEqual(writes.map(write => write.path.replace(/\?.*/, '')), ['/chats', '/chats/dm-finance/objects', '/chats/dm-priya/messages/p2/forward', '/chats/dm-priya/messages/p2/forward']);
    assert.deepEqual(writes[0].body.agents, ['finance'], 'the chat it is stored in comes first');
    const [first, second] = forwards(writes).map(write => write.body);
    assert.deepEqual(first.files, ['inv']);
    assert.deepEqual(first.attachments, ['stored-policy.pdf']);
    assert.deepEqual(second, first, 'the same forward, with the same stored document');
    assert.equal(first.chat, writes[0].body.id, 'one ID for the chat, whoever creates it');
    assert.equal(dialog(), undefined);
  }, { url, fetch: server(writes, { fail: 1 }) });
});

test('a message that is only files forwards its files, and needs one of them', async () => {
  const writes = [];
  const shared = { ...asked, id: 'p3', content: 'Shared files', attachments: [pdf('inv', 'invoice.pdf')] };
  await reactFixture(async ({ render, click, change }) => {
    await show(render, [shared]);
    await click(button(article('p3'), 'Forward'));
    assert.equal(dialog().querySelector('.chat-forwarded').textContent.includes('Shared files'), false, 'no placeholder text');
    await search(change, click, 'Priya Sharma');
    await click(button(dialog(), 'Leave out invoice.pdf'));
    assert.equal(button(dialog(), 'Forward to Priya Sharma').disabled, true);
    assert.match(dialog().textContent, /Include a file to forward/);
    await click(button(dialog(), 'Include invoice.pdf'));
    await click(button(dialog(), 'Forward to Priya Sharma'));
    assert.deepEqual(writes.map(write => write.path), ['/chats/dm-priya/messages/p3/forward'], 'no upload and no new chat for a message’s own files');
    assert.deepEqual(writes[0].body.files, ['inv']);
  }, { url, fetch: server(writes) });
});

test('a forward shows the files it carries inside the copy and the sender’s own below', async () => {
  const copy = { name: 'Priya Sharma', kind: 'user', place: 'Direct message', at: 1763283720, text: '', files: [{ id: 'c1', name: 'invoice.pdf', mime: 'application/octet-stream', size: 2048 }] };
  const list = [{ id: 'f3', chat: 'dm-priya', author: 'ravi', name: 'Ravi Kumar', kind: 'user', content: 'Is this paid?', state: 'sent', at: 4, forwarded: copy, attachments: [pdf('c1', 'invoice.pdf'), pdf('o1', 'policy.pdf')] }];
  await reactFixture(async ({ render }) => {
    await show(render, list);
    const block = article('f3').querySelector('.chat-forwarded');
    assert.match(block.textContent, /Forwarded from Priya Sharma/);
    assert.match(block.textContent, /invoice\.pdf/);
    assert.equal(block.textContent.includes('policy.pdf'), false, 'your own document is not part of the copy');
    assert.equal(block.querySelector('.message-markdown'), null, 'a copy that only shared files has no text');
    assert.match(article('f3').textContent, /policy\.pdf/);
  }, { url, fetch: server([]) });
});

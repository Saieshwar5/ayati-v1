import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement, Fragment } from 'react';
import { reactFixture } from './react-fixture.mjs';

// Forwarding a message (schema 125): pick a person or agent, say what you need,
// send once, and land in your chat with them. A forward shows its copy.
const w = '11111111-1111-4111-8111-111111111111', url = `http://localhost/w/${w}/?view=chat&conversation=dm-priya`;
const session = { user: { id: 'ravi', email: 'ravi@example.test' }, workspaces: [{ id: w, name: 'Northwind', role: 'Member' }] };
const directory = { role: 'Member',
  users: [{ id: 'ravi', name: 'Ravi Kumar', title: '', photo: null }, { id: 'priya', name: 'Priya Sharma', title: 'Head of Operations', photo: null }],
  agents: [{ id: 'finance', name: 'Finance Desk', role: 'Invoices and payments', audience: 'everyone' },
    { id: 'mine', name: 'My assistant', role: 'Personal assistant', audience: 'only_me', yours: true },
    { id: 'theirs', name: 'Priya’s assistant', role: 'Personal assistant', audience: 'only_me' }] };
const asked = { id: 'p1', chat: 'dm-priya', author: 'priya', name: 'Priya Sharma', kind: 'user', content: 'Acme says invoice INV-2291 is unpaid.', state: 'sent', at: 1763283720 };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
function server(writes, { fail = 0, hold } = {}) {
  return async (path, options = {}) => {
    if (options.method === 'POST') {
      writes.push({ path: path.replace(`/api/identity/workspaces/${w}`, ''), body: JSON.parse(options.body) });
      if (writes.length <= fail) return response({ error: 'The agent is busy' }, 409);
      return hold ? new Promise(resolve => { hold.release = () => resolve(response({ ok: true, chat: 'dm-finance' })); }) : response({ ok: true, chat: 'dm-finance' });
    }
    if (path.endsWith('/session')) return response(session);
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
const choose = async (click, name) => click([...dialog().querySelectorAll('.chat-forward-list button')].find(b => b.textContent.includes(name)));

test('forwarding to an agent needs your words, sends once and opens your chat with it', async () => {
  const writes = [], hold = {};
  await reactFixture(async ({ render, click, change, update }) => {
    await show(render);
    await click(button(article('p1'), 'Forward'));
    assert.ok(dialog(), 'the forward window opens');
    const names = [...dialog().querySelectorAll('.chat-forward-list button')].map(b => b.textContent);
    assert.ok(names.some(name => name.includes('Finance Desk')) && names.some(name => name.includes('My assistant')));
    assert.equal(names.some(name => name.includes('Priya’s assistant')), false, 'not someone else’s only-me agent');
    assert.equal(names.some(name => name.includes('Ravi Kumar')), false, 'not yourself');
    assert.match(dialog().querySelector('.chat-forwarded').textContent, /Forwarded from Priya Sharma.*Direct message/);
    await choose(click, 'Finance Desk');
    assert.match(dialog().textContent, /owners and admins can see what you forward/);
    assert.equal(button(dialog(), 'Forward to Finance Desk').disabled, true, 'an agent needs to know what you need');
    await change(dialog().querySelector('textarea'), 'Find out why this is unpaid.');
    const send = button(dialog(), 'Forward to Finance Desk');
    assert.equal(send.disabled, false);
    await update(() => { send.click(); send.click(); });
    assert.equal(writes.length, 1, 'one request despite a double click');
    assert.equal(writes[0].path, '/chats/dm-priya/messages/p1/forward');
    assert.deepEqual({ ...writes[0].body, id: undefined, chat: undefined }, { id: undefined, chat: undefined, to: { kind: 'agent', id: 'finance' }, note: 'Find out why this is unpaid.' });
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
    await choose(click, 'Priya Sharma');
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

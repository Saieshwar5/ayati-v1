import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement, useState } from 'react';
import { reactFixture } from './react-fixture.mjs';
import { decodeMentions } from '../src/react/chat/mentions.ts';

// Asking your own assistant (2026-11-21): what a message box asks it, which message a bare
// "respond" answers, and the @ list. The assistant here is named Tara: a mention holds its
// ID, so nothing depends on the name. The request and its draft are in assistant-ask.test.mjs.
const tara = { id: 'tara', name: 'Tara', kind: 'agent' };
const mention = '[@Tara](mention:agent:tara)';
const said = (id, author, content, sequence) => ({ id, author, name: author, kind: 'user', content, state: 'sent', at: sequence, sequence });
const karthik = said('m1', 'karthik', 'How are November sales going against target?', 1);

test('what the box asks is its text without the assistant, and a typed name is not a mention', async () => {
  const { assistantRequest, answersLatest, latestForYou, mentionAssistant, mentionTyped, namesAssistant } = await import('../src/react/chat/assistant-ask-model.ts');
  assert.equal(assistantRequest('Hello [@Sam](mention:user:u1)', 'tara'), undefined, 'an ordinary message');
  assert.equal(assistantRequest(`${mention} go through sales and answer [@Karthik](mention:user:k1)`, 'tara'), 'go through sales and answer @Karthik');
  assert.equal(assistantRequest(`Please ${mention} respond`, 'tara'), 'Please respond', 'the mention may be anywhere');
  assert.equal(assistantRequest(mention, 'tara'), '');
  assert.equal(assistantRequest('[@Iris](mention:agent:iris) stock?', 'tara'), undefined, 'a team agent is asked in public, as before');
  for (const bare of ['', 'respond', 'Reply.', 'please answer', 'respond to this']) assert.ok(answersLatest(bare), bare);
  for (const told of ['respond that Friday works', 'go through sales']) assert.ok(!answersLatest(told), told);

  const mine = said('m3', 'ravi', 'On it.', 3), named = said('m2', 'meera', 'Can [@Ravi](mention:user:ravi) confirm?', 2), last = said('m4', 'meera', 'Lunch?', 4);
  assert.equal(latestForYou([karthik, named, mine, last], 'ravi', false).id, 'm2', 'in a channel: the newest that names you');
  assert.equal(latestForYou([karthik, named, mine, last], 'ravi', true).id, 'm4', 'in a direct chat: their newest');
  assert.equal(latestForYou([karthik, mine], 'ravi', false).id, 'm1', 'nobody named you: the newest from someone else');
  assert.equal(latestForYou([mine], 'ravi', false), undefined);

  const first = mentionAssistant('see [@Sam](mention:user:u1)', tara);
  assert.equal(decodeMentions(first).text, '@Tara see @Sam');
  assert.deepEqual(decodeMentions(first).mentions.map(m => m.id), ['tara', 'u1'], 'other mentions keep their identity');
  assert.ok(namesAssistant('@tara tell him wholesale is behind', tara), 'typed by hand, whatever the case');
  assert.ok(!namesAssistant('@Tarantino is here', tara) && !namesAssistant('ask @Tara', tara) && !namesAssistant(`${mention} hi`, tara));
  assert.equal(assistantRequest(mentionTyped(' @tara tell him', tara), 'tara'), 'tell him');
});

test('your assistant is first in the @ list under any name, and mentioning it turns Send into Ask', async () => {
  await reactFixture(async ({ render, change, click }) => {
    const { Composer } = await import('../src/react/chat/composer.tsx');
    const iris = { id: 'iris', name: 'Iris', kind: 'agent' }, karthik = { id: 'k1', name: 'Karthik Rao', kind: 'user', title: 'Sales lead' };
    let sent = 0;
    function Harness() { const [value, set] = useState(''); return createElement(Composer, { value, change: set, send: () => { sent++; }, candidates: [karthik, iris], assistant: tara }); }
    await render(Harness);
    const input = document.querySelector('textarea');
    const options = () => [...document.querySelectorAll('[role=option]')].map(option => `${option.querySelector('.chat-name-text').textContent}: ${option.querySelector('small').textContent}`);
    await change(input, '@');
    assert.deepEqual(options(), ['Tara: Your assistant · Only you see what you ask', 'Karthik Rao: Sales lead', 'Iris: Asks this agent to work']);
    for (const typed of ['@my', '@assist', '@ta']) { await change(input, typed); assert.equal(options()[0].split(':')[0], 'Tara', typed); }
    await change(input, '@kar'); assert.deepEqual(options(), ['Karthik Rao: Sales lead'], 'not offered for someone else’s name');
    assert.equal(document.querySelector('button[type=submit]').getAttribute('aria-label'), 'Send message');
    await change(input, '@ta'); await click(document.querySelector('[role=option]'));
    const form = document.querySelector('form');
    assert.match(form.className, /is-asking/);
    assert.equal(document.querySelector('button[type=submit]').textContent, 'Ask Tara');
    assert.equal(document.querySelector('button[type=submit]').getAttribute('aria-label'), 'Ask Tara');
    await change(input, '@Tara ask @ir');
    assert.deepEqual(options(), ['Iris: Named only, not asked'], 'a team agent named in what you ask is not asked');
    // Editing inside the mention makes it ordinary text again: Send posts, so it says Send.
    await change(input, '@Tar tell him');
    assert.doesNotMatch(form.className, /is-asking/);
    assert.equal(document.querySelector('button[type=submit]').getAttribute('aria-label'), 'Send message');
    assert.equal(sent, 0);
  });
});

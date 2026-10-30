import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement, useState } from 'react';
import { reactFixture } from './react-fixture.mjs';
const w = '11111111-1111-4111-8111-111111111111', url = `http://localhost/w/${w}/?view=chat`;
const session = { user: { id: 'requester', email: 'requester@example.test' }, workspaces: [{ id: w, name: 'Store', role: 'Admin' }] };
const spec = { version: 1, kind: 'form', title: 'Quotation details', description: 'Supply missing facts.', fallback: 'Please supply quotation details.', options: [], allow_other: false,
  fields: [{ id: 'quantity', label: 'Quantity', kind: 'number', required: true, suggested: 0, min: 0, max: 100, description: '', options: [] },
    { id: 'date', label: 'Delivery date', kind: 'date', required: true, suggested: '2028-02-29', description: '', options: [] },
    { id: 'express', label: 'Express delivery', kind: 'boolean', required: true, suggested: false, description: '', options: [] },
    { id: 'notes', label: 'Notes', kind: 'multiline', required: false, description: '', options: [] }] };
const question = { id: 'q', key: 'quotation', revision: 1, state: 'pending', values: null, specification: spec, respondent: 'requester', respondent_name: 'Ananya', can_respond: true, reply: 'reply', chat: 'source' };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
async function setup() { const context = await import('../src/workspace-context.ts'); await context.enterWorkspace(); return (await import('../src/react/questions/dialog.tsx')).QuestionDialog; }

test('forms submit explicitly, guard double sends, preserve failed drafts and reuse only the exact submission', async () => {
  let finish; const bodies = [];
  await reactFixture(async ({ render, button, click, change, submit, update }) => {
    const Dialog = await setup(); await render(Dialog, { id: 'q', close: () => {} });
    assert.equal(bodies.length, 0);
    assert.equal(document.querySelector('input[type=number]').value, '0');
    assert.equal(document.querySelector('select').value, 'false');
    await change(document.querySelector('textarea'), 'Keep my draft');
    await update(() => { const form = document.querySelector('form'); form.dispatchEvent(new window.Event('submit', { bubbles: true, cancelable: true })); form.dispatchEvent(new window.Event('submit', { bubbles: true, cancelable: true })); });
    assert.equal(bodies.length, 1); assert.deepEqual(bodies[0].values, { quantity: 0, date: '2028-02-29', express: false, notes: 'Keep my draft' });
    await update(() => finish(response({ error: 'Save temporarily unavailable' }, 503)));
    assert.equal(document.querySelector('textarea').value, 'Keep my draft');
    assert.match(document.body.textContent, /Save temporarily unavailable/);
    await submit(document.querySelector('form')); assert.equal(bodies[0].id, bodies[1].id);
    await update(() => finish(response({ error: 'Still unavailable' }, 503)));
    await change(document.querySelector('input[type=number]'), '12'); await click(button('Submit answer'));
    assert.notEqual(bodies[2].id, bodies[0].id);
    await update(() => finish(response({ id: 'q', revision: 1, state: 'answered', values: bodies[2].values })));
    assert.equal(document.querySelector('form'), null); assert.match(document.body.textContent, /Answer saved/);
    assert.match(document.querySelector('.question-answer').textContent, /No/);
  }, { url, fetch: async (path, options = {}) => {
    if (options.method === 'POST') { bodies.push(JSON.parse(options.body)); return new Promise(resolve => { finish = resolve; }); }
    return response(path.endsWith('/session') ? session : question);
  } });
});

test('closed single choice and explicit multiple alternatives submit saved option IDs without executing on selection', async () => {
  const submitted = [];
  await reactFixture(async ({ render, update, change, button, click }) => {
    const { QuestionForm } = await import('../src/react/questions/form.tsx');
    const options = [{ id: 'a', label: 'Air' }, { id: 'r', label: 'Road' }];
    const props = { question: { ...question, specification: { ...spec, kind: 'single_choice', options, fields: [] } }, busy: false, respond: (decision, values) => submitted.push({ decision, values }) };
    await render(QuestionForm, props); assert.equal(document.querySelector('input[type=text]'), null);
    await update(() => document.querySelector('input[type=radio]').click()); assert.equal(submitted.length, 0);
    await click(button('Submit answer')); assert.deepEqual(submitted[0], { decision: 'answer', values: { choices: ['a'] } });
    await render(QuestionForm, { ...props, question: { ...question, specification: { ...spec, kind: 'multiple_choice', options, fields: [], allow_other: true, max_choices: 2 } } });
    await update(() => { document.querySelectorAll('input[type=checkbox]')[2].click(); });
    await change(document.querySelector('input[type=text]'), 'Train'); assert.equal(submitted.length, 1);
    await click(button('Submit answer')); assert.deepEqual(submitted[1], { decision: 'answer', values: { choices: ['a'], other: 'Train' } });
    await click(button('Decline question')); assert.deepEqual(submitted[2], { decision: 'decline', values: {} });
  });
});

test('a viewer cannot answer for another requester and opening a question is read-only', async () => {
  const writes = [];
  await reactFixture(async ({ render }) => {
    const Dialog = await setup(); await render(Dialog, { id: 'q', close: () => {} });
    assert.equal(document.querySelector('form'), null); assert.match(document.body.textContent, /Only the intended requester/); assert.deepEqual(writes, []);
  }, { url, fetch: async (path, options = {}) => { if (options.method === 'POST') writes.push(path); return response(path.endsWith('/session') ? session : { ...question, respondent: 'other', can_respond: false }); } });
});

test('prototype-named field IDs remain ordinary data and never supply inherited values', async () => {
  const answers = [];
  await reactFixture(async ({ render, change, button, click }) => {
    const { QuestionForm } = await import('../src/react/questions/form.tsx');
    const fields = ['constructor', '__proto__'].map(id => ({ id, label: id, kind: 'text', required: false, description: '', options: [] }));
    await render(QuestionForm, { question: { ...question, specification: { ...spec, fields } }, busy: false, respond: (_decision, values) => answers.push(values) });
    const inputs = [...document.querySelectorAll('input[type=text]')]; assert.ok(inputs.every(input => input.value === ''));
    await click(button('Submit answer')); assert.deepEqual(answers[0], {});
    await change(inputs[0], 'Builder'); await change(inputs[1], 'Ordinary text'); await click(button('Submit answer'));
    assert.equal(Object.hasOwn(answers[1], '__proto__'), true); assert.equal(answers[1].constructor, 'Builder');
    assert.equal(answers[1].__proto__, 'Ordinary text'); assert.equal(Object.getPrototypeOf(answers[1]), Object.prototype);
  });
});

test('denied refresh clears a previously visible question and its draft', async () => {
  let denied = false;
  await reactFixture(async ({ render, change, button, click }) => {
    const Dialog = await setup(); await render(Dialog, { id: 'q', close: () => {} });
    await change(document.querySelector('textarea'), 'PRIVATE_UNSENT_FACT'); denied = true;
    await click(button('Refresh question'));
    assert.equal(document.querySelector('textarea'), null); assert.equal(document.body.textContent.includes('PRIVATE_UNSENT_FACT'), false);
    assert.match(document.querySelector('[role=alert]').textContent, /Source access denied/);
  }, { url, fetch: async path => path.endsWith('/session') ? response(session) : denied ? response({ error: 'Source access denied' }, 403) : response(question) });
});

test('late reads from a different question cannot replace the current record or reveal old fields', async () => {
  const pending = [];
  await reactFixture(async ({ render, update }) => {
    const Dialog = await setup(); await render(Dialog, { id: 'old', close: () => {} });
    await render(Dialog, { id: 'new', close: () => {} });
    await update(() => pending.forEach(finish => finish(response({ ...question, specification: { ...spec, title: 'STALE_PRIVATE_QUESTION' } }))));
    assert.match(document.body.textContent, /Current question/); assert.equal(document.body.textContent.includes('STALE_PRIVATE_QUESTION'), false);
  }, { url, fetch: async path => path.endsWith('/session') ? response(session) : path.endsWith('/questions/old') ? new Promise(resolve => pending.push(resolve)) : response({ ...question, id: 'new', specification: { ...spec, title: 'Current question' } }) });
});

test('unsupported versions show a readable fallback without active input controls', async () => {
  await reactFixture(async ({ render }) => {
    const Dialog = await setup(); await render(Dialog, { id: 'q', close: () => {} });
    assert.equal(document.querySelector('form'), null); assert.match(document.body.textContent, /Please supply quotation details/);
  }, { url, fetch: async path => response(path.endsWith('/session') ? session : { ...question, specification: { ...spec, version: 2 } }) });
});

test('Escape dismisses only the top question dialog inside an existing thread dialog', async () => {
  let outerClosed = 0;
  await reactFixture(async ({ render, update }) => {
    const QuestionDialog = await setup(); const { Dialog } = await import('../src/react/ui.tsx');
    function Harness() {
      const [opened, open] = useState(true);
      return createElement(Dialog, { title: 'Thread', close: () => { outerClosed++; } }, opened && createElement(QuestionDialog, { id: 'q', close: () => open(false) }));
    }
    await render(Harness);
    const inner = [...document.querySelectorAll('dialog')].find(node => node.querySelector('h2').textContent === 'Quotation details');
    await update(() => inner.dispatchEvent(new window.Event('cancel', { bubbles: true, cancelable: true })));
    assert.equal(outerClosed, 0); assert.equal(document.querySelectorAll('dialog').length, 1);
    assert.match(document.querySelector('dialog').textContent, /Thread/);
  }, { url, fetch: async path => response(path.endsWith('/session') ? session : question) });
});

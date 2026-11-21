import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { reactFixture } from './react-fixture.mjs';

// A member's assistant is set up on its agent page, like every agent: what it knows
// about you (its memory) and its rules in place of an instructions box, regular checks beside its
// schedules. The notes are its person's alone; an admin sees rules and skills only.
const w = '11111111-1111-4111-8111-111111111111', url = `http://localhost/w/${w}/?view=agents`;
const session = { user: { id: 'ravi', email: 'ravi@example.test' }, workspaces: [{ id: w, name: 'Northwind', role: 'Member' }] };
const settings = { provider: '', instructions: 'Never reply to a customer without asking me.', escalation: '', avatar: 'spark', memory: false, browser: false, history: 'all', paused: false };
const mine = { id: 'ayati', name: 'Ayati', role: '', model: 'fixture', audience: 'only_me', revision: 3, settings, editable: true, assistant: true, is_owner: true, owner_name: 'Ravi' };
const directory = { role: 'Member', agents: [mine], default_model: 'fixture', providers: [], system_prompt: 'System guide' };
const noAlways = { items: [], given: [], hidden: 0, chosen: 0, editable: false, most: 5, budget: 8000 };
const stored = { checks: false, every_minutes: 240, checklist: '', learn: true };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
function server(writes, { fail = 0, hold, memory = { memories: [], ended: [], most: 500 } } = {}) {
  return async (path, options = {}) => {
    const short = path.replace(`/api/identity/workspaces/${w}`, '');
    if (options.method === 'POST') {
      writes.push({ path: short, body: JSON.parse(options.body) });
      if (hold && writes.length <= 2) await hold;
      if (writes.length <= fail) return response({ error: 'That is too long to keep' }, 400);
      return response(short === '/agent-profiles' ? { ...mine, revision: mine.revision + writes.length } : { ok: true });
    }
    if (path.endsWith('/session')) return response(session);
    if (short === '/assistant/settings') return response(stored);
    if (short === '/assistant/memories') return response(memory);
    if (short.includes('/library-always/')) return response(noAlways);
    if (short.includes('/skills')) return response({ skills: [], editable: true });
    if (short.includes('/event-triggers')) return response({ choices: [], triggers: [], apps: [], execution_available: true });
    if (short.includes('/schedules')) return response({ schedules: [], conversations: [], available: true });
    if (short.includes('/requests')) return response({ requests: [], next_before: null });
    return response(directory);
  };
}
const button = label => [...document.querySelectorAll('button')].find(b => b.textContent.trim() === label);
async function enter() { await (await import('../src/workspace-context.ts')).enterWorkspace(); }

test('memory lists what your assistant remembers; you add a line once, change, remove and bring lines back', async () => {
  const writes = []; let release; const hold = new Promise(done => { release = done; });
  const memory = { memories: [
    { id: 'm1', kind: 'about', text: 'Head of sales.', source: 'you', created_ms: 1, ended_ms: null },
    { id: 'm2', kind: 'style', text: 'Short replies, no sign-off.', source: 'review', created_ms: 2, ended_ms: null }],
  ended: [{ id: 'm3', kind: 'work', text: 'Supplier contract renews in March.', source: 'chat', created_ms: 3, ended_ms: 4 }], most: 500 };
  await reactFixture(async ({ render, click, change, update }) => {
    await enter();
    const { AssistantMemory } = await import('../src/react/agents/assistant/memory.tsx');
    await render(() => createElement(AssistantMemory, { name: 'Ayati' }));
    const section = document.querySelector('.assistant-memory');
    assert.deepEqual([...section.querySelectorAll('.assistant-memory-group h4')].map(node => node.textContent), ['About you', 'How you write']);
    assert.match(section.textContent, /Short replies, no sign-off\.Ayati learned this looking back at your work/);
    assert.match(section.textContent, /Only you can read this/);

    // A new line: one save despite a double submit; a failed save keeps your words and its id.
    const input = () => section.querySelector('input[aria-label="Something to remember"]');
    await change(section.querySelector('select[aria-label="What the line is about"]'), 'people');
    await change(input(), ' Arjun approves  discounts. ');
    const form = section.querySelector('.assistant-memory-add');
    const send = () => form.dispatchEvent(new window.Event('submit', { bubbles: true, cancelable: true }));
    await update(() => { send(); send(); });
    release();
    await update(() => hold);
    assert.equal(writes.length, 1);
    assert.match(section.textContent, /too long to keep/);
    assert.equal(input().value, ' Arjun approves  discounts. ');
    await update(send);
    assert.equal(writes[1].path, '/assistant/memories');
    assert.deepEqual({ ...writes[1].body, id: undefined }, { id: undefined, kind: 'people', text: 'Arjun approves discounts.' });
    assert.equal(writes[1].body.id, writes[0].body.id, 'the retry is the same line, kept once');
    assert.equal(input().value, '');

    // Change one, remove one, bring back one the assistant ended.
    await click(section.querySelector('button[aria-label="Edit: Head of sales."]'));
    await change(section.querySelector('input[aria-label="Remembered line"]'), 'Head of sales, India.');
    await click(button('Save'));
    assert.deepEqual(writes[2], { path: '/assistant/memories', body: { id: 'm1', kind: 'about', text: 'Head of sales, India.' } });
    assert.equal(section.querySelector('input[aria-label="Remembered line"]'), null);
    await click(section.querySelector('button[aria-label="Remove: Short replies, no sign-off."]'));
    assert.equal(writes[3].path, '/assistant/memories/m2/remove');
    assert.match(section.querySelector('.assistant-memory-ended summary').textContent, /Ayati ended 1 line/);
    await click(section.querySelector('button[aria-label="Bring back: Supplier contract renews in March."]'));
    assert.equal(writes[4].path, '/assistant/memories/m3/restore');

    // Learning on its own is on until its person turns it off, saved at once.
    await click(section.querySelector('input[type=checkbox]'));
    assert.deepEqual(writes[5], { path: '/assistant/settings', body: { learn: false } });
    assert.match(section.textContent, /only remembers what you add or tell it/);
  }, { url, fetch: server(writes, { fail: 1, hold, memory }) });
});

test('regular checks save their own fields and show the checklist only when on', async () => {
  const writes = [];
  await reactFixture(async ({ render, click, change }) => {
    await enter();
    const { RegularChecks } = await import('../src/react/agents/assistant/regular-checks.tsx');
    await render(() => createElement(RegularChecks, { name: 'Ayati' }));
    assert.equal(document.querySelector('textarea'), null);
    await click(document.querySelector('input[type=checkbox]'));
    await change(document.querySelector('select'), '60');
    await change(document.querySelector('textarea'), 'Any late orders?');
    await click(button('Save changes'));
    assert.deepEqual(writes, [{ path: '/assistant/settings', body: { checks: true, every_minutes: 60, checklist: 'Any late orders?' } }]);
    assert.match(document.body.textContent, /Regular checks are on/);
  }, { url, fetch: server(writes) });
});

test('rules are a short list: one is added once, kept on a failed save, and removed', async () => {
  const writes = []; let release; const hold = new Promise(done => { release = done; });
  await reactFixture(async ({ render, click, change, update, submit }) => {
    await enter();
    const { DoesRow } = await import('../src/react/agents/profile/profile-rows.tsx');
    await render(() => createElement(DoesRow, { agent: mine, directory, open() {}, show() {}, setDraft() {}, saved: async () => {} }));
    assert.equal(document.querySelector('.profile-row-label').textContent, 'Rules', 'an assistant’s first row is its rules');
    const rules = () => [...document.querySelectorAll('.assistant-rule-list li span')].map(node => node.textContent);
    const input = () => document.querySelector('input[aria-label="New rule"]');
    assert.deepEqual(rules(), ['Never reply to a customer without asking me.']);
    assert.equal([...document.querySelectorAll('textarea')].some(area => /Ground rules/.test(area.closest('label')?.textContent ?? '')), false, 'no instructions box');
    await change(input(), '  Keep replies\tunder five lines. ');
    const form = document.querySelector('.assistant-rule-add');
    const send = () => form.dispatchEvent(new window.Event('submit', { bubbles: true, cancelable: true }));
    await update(() => { send(); send(); });
    release();
    await update(() => hold);
    assert.equal(writes.length, 1, 'one save despite a double submit');
    assert.match(document.body.textContent, /too long to keep/);
    assert.equal(input().value, '  Keep replies\tunder five lines. ', 'what you typed stays');
    assert.equal(rules().length, 1);
    await update(() => submit(form));
    assert.equal(writes[1].body.settings.instructions, 'Never reply to a customer without asking me.\nKeep replies under five lines.');
    assert.equal(writes[1].body.revision, 3);
    assert.equal(input().value, '');
    assert.equal(rules().length, 2);
    await click(document.querySelector('button[aria-label="Remove rule: Never reply to a customer without asking me."]'));
    assert.equal(writes[2].body.settings.instructions, 'Keep replies under five lines.');
    assert.equal(writes[2].body.revision, 5, 'each save continues from the revision the last one returned');
    assert.deepEqual(rules(), ['Keep replies under five lines.']);
  }, { url, fetch: server(writes, { fail: 1, hold }) });
});

test('your assistant’s page holds what it remembers about you and its regular checks; an admin sees neither on another member’s', async () => {
  const reads = [];
  const count = base => async (path, options) => { reads.push(path); return base(path, options); };
  await reactFixture(async ({ render, button, click }) => {
    await enter();
    const { AgentWorkspace } = await import('../src/react/agents/workspace.tsx');
    const page = agent => () => createElement(AgentWorkspace, { agent, directory, back() {}, openChat() {}, busy: false, saved: async () => {} });
    await render(page(mine));
    assert.deepEqual([...document.querySelectorAll('.profile-row-label')].map(node => node.textContent), ['Rules', 'Knows', 'Can use', 'On its own', 'People', 'Model']);
    assert.equal(document.querySelector('[aria-label=People] .profile-chip').textContent, 'Only you');
    assert.equal(document.querySelector('.assistant-memory'), null, 'its memory is read when opened');
    await click(button('About you'));
    assert.match(document.querySelector('.profile-panel').textContent, /What Ayati remembers about you and your work/);
    await click(button('Regular checks'));
    assert.ok(document.querySelector('.profile-panel .assistant-checks'), 'regular checks sit with its schedules and triggers');
    reads.length = 0;
    const theirs = { ...mine, id: 'priyas', is_owner: false, owner_name: 'Priya', can_talk: false };
    await render(page(theirs));
    assert.equal(button('About you'), undefined);
    assert.equal(button('Regular checks'), undefined);
    assert.equal(document.querySelector('[aria-label=People] .profile-chip').textContent, 'Only Priya');
    assert.equal(reads.some(path => /\/assistant\/(settings|memories)$/.test(path)), false, 'the viewer’s own memory is never read for someone else’s assistant');
  }, { url, fetch: count(server([])) });
});

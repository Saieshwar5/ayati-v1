import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { reactFixture } from './react-fixture.mjs';

// A member's assistant is set up on its agent page, like every agent: what it knows
// about you and its rules in place of an instructions box, regular checks beside its
// schedules. The notes are its person's alone; an admin sees rules and skills only.
const w = '11111111-1111-4111-8111-111111111111', url = `http://localhost/w/${w}/?view=agents`;
const session = { user: { id: 'ravi', email: 'ravi@example.test' }, workspaces: [{ id: w, name: 'Northwind', role: 'Member' }] };
const settings = { provider: '', instructions: 'Never reply to a customer without asking me.', escalation: '', avatar: 'spark', memory: false, browser: false, history: 'all', paused: false };
const mine = { id: 'ayati', name: 'Ayati', role: '', model: 'fixture', audience: 'only_me', revision: 3, settings, editable: true, assistant: true, is_owner: true, owner_name: 'Ravi' };
const directory = { role: 'Member', agents: [mine], default_model: 'fixture', providers: [], system_prompt: 'System guide' };
const noAlways = { items: [], given: [], hidden: 0, chosen: 0, editable: false, most: 5, budget: 8000 };
const stored = { about: 'Head of sales.', style: '', checks: false, every_minutes: 240, checklist: '' };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
function server(writes, { fail = 0, hold } = {}) {
  return async (path, options = {}) => {
    const short = path.replace(`/api/identity/workspaces/${w}`, '');
    if (options.method === 'POST') {
      writes.push({ path: short, body: JSON.parse(options.body) });
      if (hold) await hold;
      if (writes.length <= fail) return response({ error: 'That is too long to keep' }, 400);
      return response(short === '/agent-profiles' ? { ...mine, revision: mine.revision + writes.length } : { ok: true });
    }
    if (path.endsWith('/session')) return response(session);
    if (short === '/assistant/settings') return response(stored);
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

test('what your assistant knows about you saves only those notes; a failed save keeps your words', async () => {
  const writes = [];
  await reactFixture(async ({ render, click, change }) => {
    await enter();
    const { AboutYou } = await import('../src/react/agents/assistant/about-you.tsx');
    await render(() => createElement(AboutYou, { name: 'Ayati' }));
    const areas = () => [...document.querySelectorAll('textarea')];
    assert.equal(areas()[0].value, 'Head of sales.');
    assert.equal(button('Save changes').disabled, true, 'nothing to save yet');
    await change(areas()[1], 'Short and friendly.');
    await click(button('Save changes'));
    assert.match(document.body.textContent, /too long to keep/);
    assert.equal(areas()[1].value, 'Short and friendly.');
    await click(button('Save changes'));
    assert.deepEqual(writes[1], { path: '/assistant/settings', body: { about: 'Head of sales.', style: 'Short and friendly.', learn: true } }, 'regular checks are not sent, so they are kept');
    // Learning from edits is on until its person turns it off.
    await click(document.querySelector('input[type=checkbox]'));
    await click(button('Save changes'));
    assert.equal(writes[2].body.learn, false);
    assert.match(document.body.textContent, /Saved\./);
    assert.equal(button('Save changes').disabled, true);
  }, { url, fetch: server(writes, { fail: 1 }) });
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
    const { AssistantAboutTab } = await import('../src/react/agents/assistant/about-tab.tsx');
    await render(() => createElement(AssistantAboutTab, { agent: mine, rules: 'System guide', saved: async () => {} }));
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

test('your assistant’s page names its tab About you; an admin sees another member’s rules and skills but not their notes', async () => {
  const reads = [];
  const count = base => async (path, options) => { reads.push(path); return base(path, options); };
  await reactFixture(async ({ render, button: tab }) => {
    await enter();
    const { AgentWorkspace } = await import('../src/react/agents/workspace.tsx');
    const page = (agent, section) => () => createElement(AgentWorkspace, { agent, directory, section, back() {}, openChat() {}, busy: false, selectSection() {}, saved: async () => {} });
    await render(page(mine, 'Instructions'));
    assert.deepEqual([...document.querySelectorAll('[role=tab]')].map(node => node.textContent), ['Overview', 'About you', 'Connections', 'Automations', 'Settings']);
    assert.match(document.querySelector('[role=tabpanel]').textContent, /What Ayati knows about you/);
    assert.match(document.querySelector('[role=tabpanel]').textContent, /Skills it can use/);
    await render(page(mine, 'Automations'));
    assert.ok(document.querySelector('.assistant-checks'), 'regular checks sit with its schedules and triggers');
    reads.length = 0;
    const theirs = { ...mine, id: 'priyas', is_owner: false, owner_name: 'Priya' };
    await render(page(theirs, 'Instructions'));
    assert.ok(tab('Rules and skills'));
    assert.equal(/knows about you/.test(document.body.textContent), false);
    await render(page(theirs, 'Automations'));
    assert.equal(document.querySelector('.assistant-checks'), null);
    assert.equal(reads.some(path => path.endsWith('/assistant/settings')), false, 'the viewer’s own notes are never read for someone else’s assistant');
  }, { url, fetch: count(server([])) });
});

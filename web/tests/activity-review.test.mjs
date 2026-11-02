import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { reactFixture } from './react-fixture.mjs';

// Getting better (Phase 5): Activity → Review and Home's hints.
const w = '11111111-1111-4111-8111-111111111111';
const reviewUrl = `http://localhost/w/${w}/?view=activity&tab=review`;
const session = { user: { id: 'kavya', email: 'kavya@example.test' }, workspaces: [{ id: w, name: 'Store', role: 'Member' }] };
const response = (data, status = 200) => ({ ok: status < 400, status, json: async () => data });
const now = Date.now();
const page = {
  available: true, days: 30,
  agents: [{ id: 'nova', name: 'Nova', tasks: 40, done: 36, failed: 2, open: 2, due_done: 10, on_time: 9, needed_person: 12, person_minutes: 31.5, requests: 120, model_runs: 140 }],
  people: [{ id: 'kavya', name: 'Kavya Nair', tasks: 20, done: 18, failed: 1, open: 1, due_done: 0, on_time: 0, needed_person: 6, person_minutes: 14 }],
  suggestions: [{ agent: 'nova', agent_name: 'Nova', connection: 'shop', operation: 'order.refund', title: 'Refund order', approved: 6 }],
  goals: [{ id: 'g1', agent: 'nova', agent_name: 'Nova', name: 'Quotes sent', target: 50, unit: 'quotes', period: 'week', current: 42, risk: 'Two suppliers are late', updated_ms: now }],
  reviews: [{ id: 'v1', agent: 'nova', agent_name: 'Nova', done: 'Sent 42 quotes', went_wrong: 'Two failed on Zoho', learned: 'Check the token first', suggestion: 'Add the price list skill', at: now }],
  weekly: [{ agent: 'nova', enabled: true, next_ms: now + 86400000 }],
};
async function activity() {
  const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
  return (await import('../src/react/activity/activity-page.tsx')).ActivityPage;
}

test('Review shows measures, a rule offer, goals and weekly reviews; each choice sends one request', async () => {
  const writes = [];
  await reactFixture(async ({ render, button, click, change, update }) => {
    await render(await activity(), { href: reviewUrl });
    const text = document.querySelector('[aria-label="Review"]').textContent;
    assert.match(text, /Nova asked 6 times to refund order, and each was approved/);
    assert.match(text, /36 of 40.*9 of 10.*12.*31\.5/);
    assert.match(text, /Nova: Quotes sent.*42 of 50 quotes this week · at risk: Two suppliers are late/);
    assert.match(text, /Suggested change: Add the price list skill/);
    assert.equal(document.querySelector('[role=progressbar]').getAttribute('aria-valuenow'), '84');
    await click(button('Let it'));
    assert.deepEqual(writes.at(-1).body, { agent: 'nova', connection: 'shop', operation: 'order.refund', accept: true });
    await click(button('Weekly review for Nova'));
    assert.deepEqual(writes.at(-1).body, { agent: 'nova', enabled: false });
    const form = document.querySelector('.activity-goal-form');
    await change(form.querySelector('input[aria-label="Goal"]'), 'Orders packed');
    await change(form.querySelector('input[aria-label="Target"]'), '200');
    await update(() => form.dispatchEvent(new window.Event('submit', { bubbles: true, cancelable: true })));
    assert.deepEqual(writes.at(-1).body, { agent: 'nova', name: 'Orders packed', target: 200, unit: '', period: 'week' });
    assert.ok(writes.at(-1).path.endsWith('/activity/goals'));
  }, { url: reviewUrl, fetch: async (path, options = {}) => {
    if (path.endsWith('/session')) return response(session);
    if (options.method === 'POST') { writes.push({ path, body: JSON.parse(options.body) }); return response({ ok: true }); }
    if (path.includes('/attention')) return response({ available: true, items: [], next_before: null });
    if (path.includes('/activity/review')) return response(page);
    return response({});
  } });
});

test('Home says how many things need you in one line, and suggests a panel for a question asked again and again', async () => {
  await reactFixture(async ({ render, button, click }) => {
    const context = await import('../src/workspace-context.ts'); await context.enterWorkspace();
    const { NeedsYouLine, PanelSuggestions } = await import('../src/react/home-panels/home-hints.tsx');
    const items = [{ kind: 'approval', id: 'a' }, { kind: 'question', id: 'q' }, { kind: 'outcome', id: 'o' }];
    await render(() => createElement('div', null, createElement(NeedsYouLine, { page: { available: true, items, next_before: null } }), createElement(PanelSuggestions)));
    assert.match(document.querySelector('.home-needs-line').textContent, /^2 things need you/);
    assert.match(document.querySelector('.home-needs-line').href, /tab=needs/);
    await click(button('Not now'));
    assert.equal(document.querySelector('.home-panel-suggestions'), null, 'gone on this device');
  }, { url: `http://localhost/w/${w}/?view=home`, fetch: async path => path.endsWith('/session') ? response(session)
    : path.includes('/home-panels/suggestions') ? response({ suggestions: [{ agent: 'nova', agent_name: 'Nova', ask: 'How many orders today?', count: 4 }] }) : response({}) });
});

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });

// Agents that improve (schema 143): the Improving panel with proposed skills,
// and the workspace's week in Settings › Memory.
const workspace = '11111111-1111-4111-8111-111111111111';
const base = `/api/identity/workspaces/${workspace}`;
const response = (value, status = 200) => ({ ok: status < 400, status, json: async () => value });
const options = fetch => ({ url: `http://localhost/w/${workspace}/?view=agents`, fetch });
const week = (corrected, accepted) => ({ answers: 20, accepted, corrected, followed_up: 5, missed: 2, learned: 4, held: 1, skills_proposed: 1, skills_approved: 0 });
const skill = '---\nname: payment-reminder\ndescription: Use when a customer needs a reminder about an unpaid invoice\n---\n1. Find the invoice.\n';
const proposal = (state = 'proposed') => ({ id: 'p1', kind: 'new', name: 'payment-reminder', description: 'Use when…', why: 'Reminders came up 12 times on 6 days.',
  files: [{ path: 'SKILL.md', text: skill }], state, created_ms: 1794700000000, decided_ms: null, decided_by: null, job_type: 'Send a payment reminder', skill: null,
  evidence: [{ at_ms: 1794700000000, outcome: 'accepted', text: 'Remind Acme about invoice 2291', conversation: 'sales' },
    { at_ms: 1794600000000, outcome: 'corrected', text: null, conversation: null }],
  rehearsals: [{ job: 'j1', verdict: 'same', note: 'Both send the reminder.' }, { job: 'j2', verdict: 'better', note: null }] });
const improving = proposals => ({ available: true, configured: { classifier: true, reviews: true }, this_week: week(3, 81), last_week: week(7, 72),
  reviews: [{ id: 'r1', ran_ms: 1794700000000, requests: 41, job_types: 6, routine: 2, proposed: 1, result: 'Checked 41 requests in 6 job types: proposed a new skill, payment-reminder.' },
    { id: 'r0', ran_ms: 1794400000000, requests: 35, job_types: 5, routine: 0, proposed: 0, result: 'Checked 35 requests in 5 job types: no new skill needed.' }],
  jobs: [{ id: 't1', name: 'Send a payment reminder', runs: 12, accepted: 10, corrected: 2 }],
  skills: [], proposals });

test('an owner approves a proposed skill once, sees its evidence and rehearsal, and the page refreshes', async () => {
  const writes = []; let finish; let changed = 0;
  await reactFixture(async ({ render, button, update }) => {
    const { ImprovingPanel } = await import('../src/react/agents/improving/improving-panel.tsx');
    await render(ImprovingPanel, { agent: 'nova', name: 'Nova', changed: () => { changed += 1; } });
    const table = document.querySelector('table.improving-week');
    assert.match(table.textContent, /Answers corrected, per 1003 better7/);
    const card = document.querySelector('article[aria-label="New skill: payment-reminder"]');
    assert.match(card.textContent, /Created by Nova/);
    assert.match(card.textContent, /In sales.*“Remind Acme about invoice 2291”/);
    assert.match(card.textContent, /In a conversation you can’t open.*Text not shown/);
    assert.match(card.textContent, /Tried on 2 earlier requests, nothing sent or changed/);
    assert.match(card.textContent, /Better than the real answer/);
    assert.match(document.body.textContent, /no new skill needed/, 'reviews that changed nothing are listed too');
    await update(() => { button('Approve').click(); button('Approve').click(); });
    assert.equal(writes.length, 1, 'one decision for a double click');
    assert.deepEqual(writes[0], { path: `${base}/agent-profiles/nova/skill-proposals/p1`, body: { approve: true } });
    await update(() => finish(response({ state: 'approved', skill: 's1' })));
    assert.match(document.body.textContent, /payment-reminder is now one of Nova’s skills/);
    assert.equal(changed, 1);
    assert.equal(document.querySelector('article[aria-label="New skill: payment-reminder"] .improving-actions'), null);
  }, options(async (path, request = {}) => {
    if (request.body) { writes.push({ path, body: JSON.parse(request.body) }); return new Promise(resolve => { finish = resolve; }); }
    return response(improving(writes.length ? [proposal('approved')] : [proposal()]));
  }));
});

test('an edited draft is approved with its new text; a rejection says it waits 30 days; members are refused', async () => {
  const writes = [];
  await reactFixture(async ({ render, button, click, change }) => {
    const { ImprovingPanel } = await import('../src/react/agents/improving/improving-panel.tsx');
    await render(ImprovingPanel, { agent: 'nova', name: 'Nova', changed: () => {} });
    await click(button('Edit'));
    await change(document.querySelector('.improving-proposal textarea'), skill + '2. Ask a person before any discount.\n');
    await click(button('Approve edited skill'));
    assert.equal(writes[0].body.files[0].path, 'SKILL.md');
    assert.match(writes[0].body.files[0].text, /Ask a person before any discount/);
    await click(button('Reject'));
    assert.deepEqual(writes[1].body, { approve: false });
    assert.match(document.body.textContent, /won’t be proposed again for 30 days/);
  }, options(async (path, request = {}) => {
    if (request.body) { writes.push({ path, body: JSON.parse(request.body) }); return response({ state: 'ok' }); }
    return response(improving([proposal()]));
  }));
  await reactFixture(async ({ render }) => {
    const { ImprovingPanel } = await import('../src/react/agents/improving/improving-panel.tsx');
    await render(ImprovingPanel, { agent: 'nova', name: 'Nova', changed: () => {} });
    assert.match(document.querySelector('[role=alert]').textContent, /Only the agent’s owner or an admin/);
  }, options(async () => response({ error: 'Only the agent’s owner or an admin can change this agent' }, 400)));
});

test('the workspace week compares each agent with last week, and says when only plain signals count', async () => {
  await reactFixture(async ({ render }) => {
    const { WorkspaceLearning } = await import('../src/react/memory/workspace-learning.tsx');
    await render(WorkspaceLearning, {});
    assert.match(document.body.textContent, /How your agents are learning/);
    assert.match(document.body.textContent, /only, until a classifier model is set/);
    assert.match(document.body.textContent, /Nova20 answers · corrected 3 per 100, last week 7 · better/);
  }, options(async path => {
    assert.equal(path, `${base}/learning`);
    return response({ available: true, configured: { classifier: false, reviews: false }, this_week: week(4, 80), last_week: week(6, 70),
      agents: [{ id: 'nova', name: 'Nova', this_week: week(3, 81), last_week: week(7, 72) }] });
  }));
});

// Skills with real Goose (D234 slice 1, option B): the agent's instructions list
// only its assigned skills, it loads them through ayati_control at the version
// current when the request started, and every load shows in Activity.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { controllerToolsOnly } from './fixtures/worker_control.mjs';

test('an agent sees only its assigned skills and loads one through Cuckoo with real Goose', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 180000,
}, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true, capacity: 1, workspaceLimit: 1, modelLimit: 1, leaseSeconds: 15 });
  const skill = (await app.request(`${app.path}/skills`, app.cookie, { id: randomUUID(), shared: true, files: [
    { path: 'SKILL.md', text: '---\nname: quote-builder\ndescription: Use when a customer asks for a price quote\n---\n1. Read references/prices.csv.\n2. QUOTE_STEPS_FROM_SKILL\n' },
    { path: 'references/prices.csv', text: 'item,price\nteak chair,4500\n' },
  ] })).value;
  await app.request(`${app.path}/skills`, app.cookie, { id: randomUUID(), shared: true, files: [
    { path: 'SKILL.md', text: '---\nname: unassigned-skill\ndescription: NOT_ASSIGNED_DESCRIPTION\n---\nNOT_ASSIGNED_STEPS\n' },
  ] });
  await app.request(`${app.path}/agent-profiles/${app.profile.id}/skills`, app.cookie, { skill: skill.id, on: true });

  const prompt = 'Prepare a quote for 40 teak chairs';
  app.provider.setControl(prompt, { steps: [
    { tool: 'load_skill', arguments: { name: 'quote-builder' } },
    { tool: 'load_skill', arguments: { name: 'quote-builder', file: 'references/prices.csv' } },
    { tool: 'load_skill', arguments: { name: 'unassigned-skill' } },
  ] });
  const executor = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  const job = await app.send(prompt);
  await app.done(job);

  const sent = JSON.stringify(app.provider.requests);
  assert.ok(sent.includes('quote-builder: Use when a customer asks for a price quote'), 'the assigned skill is listed');
  assert.ok(!sent.includes('NOT_ASSIGNED_DESCRIPTION'), 'other skills are not');
  controllerToolsOnly(app.provider.requests.at(-1));
  const reply = app.sql(`SELECT content FROM relay_chat_messages WHERE id='${job.reply}'`);
  assert.match(reply, /QUOTE_STEPS_FROM_SKILL/);
  assert.match(reply, /teak chair,4500/);
  assert.match(reply, /not_found/);
  assert.equal(reply.includes('NOT_ASSIGNED_STEPS'), false);
  assert.equal(app.sql(`SELECT string_agg(COALESCE(path,'SKILL.md'),',' ORDER BY at_ms) FROM relay_skill_loads WHERE job='${job.reply}'`), 'SKILL.md,references/prices.csv');
  const details = (await app.request(`${app.path}/activity/requests/${job.reply}`, app.cookie)).value;
  assert.deepEqual(details.steps.map(step => step.title), ['Used skill: quote-builder (version 1)', 'Read references/prices.csv from quote-builder']);
  await app.stop(executor);
  app.pass('Real Goose lists only assigned skills, loads SKILL.md and a supporting file through ayati_control, cannot read an unassigned skill, and Activity shows each load');
});

// Agents report what they need in a fixed shape, with real Goose (people-and-agents-
// first, Phase 4): the agent calls report_problem; Activity → Now groups it under
// Needs fixing for the people who can fix the agent; dismissing clears it. The model
// is scripted; Goose, the controller and ayati_control are real.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { controllerToolsOnly } from './fixtures/worker_control.mjs';

test('an agent reports a need through report_problem and Needs fixing shows it, with real Goose', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 180000,
}, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true, capacity: 1, workspaceLimit: 1, modelLimit: 1, leaseSeconds: 15 });
  const ask = 'Refund order 4471 for the customer';
  app.provider.setControl(ask, { steps: [
    { tool: 'report_problem', arguments: { cause: 'permission', detail: 'I need refunds allowed on the Orders account', fix: 'Allow order.refund for this agent' } },
  ] });
  const executor = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  const job = await app.send(ask);
  await app.done(job);
  controllerToolsOnly(app.provider.requests.at(-1));
  assert.ok(JSON.stringify(app.provider.requests).includes('Problems: when you can'), 'the problem rule reaches the agent');
  assert.equal(app.sql(`SELECT cause||'|'||detail FROM relay_agent_needs WHERE job='${job.reply}'`), 'permission|I need refunds allowed on the Orders account');
  const today = new Date(); today.setHours(0, 0, 0, 0);
  const now = (await app.request(`${app.path}/activity/now?today=${today.getTime()}&scope=all`, app.cookie)).value;
  const group = now.fixing.find(g => g.cause === 'reported');
  assert.equal(group.title, '1 agent needs a permission');
  assert.equal(group.items[0].request, job.reply);
  assert.equal(now.agents.find(a => a.id === app.profile.id).needs_fixing, 1);
  await app.request(`${app.path}/activity/needs/${group.items[0].ref}/dismiss`, app.cookie, {});
  const after = (await app.request(`${app.path}/activity/now?today=${today.getTime()}&scope=all`, app.cookie)).value;
  assert.equal(after.fixing.filter(g => g.cause === 'reported').length, 0);
  await app.stop(executor);
  app.pass('Real Goose reports a need with report_problem; Needs fixing groups it for the agent\'s owner, and dismissing clears it');
});

test('an agent reads its goals, reports progress and a risk, and writes its weekly review, with real Goose', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 180000,
}, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true, capacity: 1, workspaceLimit: 1, modelLimit: 1, leaseSeconds: 15 });
  const goal = (await app.request(`${app.path}/activity/goals`, app.cookie, { agent: app.profile.id, name: 'Quotes sent', target: 50, unit: 'quotes', period: 'week' })).value.id;
  const ask = 'Sent the Acme quotes; also write your weekly review';
  app.provider.setControl(ask, { steps: [
    { tool: 'my_goals', arguments: {} },
    { tool: 'update_goal', arguments: { goal, current: 42, risk: 'Two suppliers are late with prices' } },
    { tool: 'write_review', arguments: { done: 'Sent 42 quotes', went_wrong: 'Two failed on Zoho', learned: 'Check the token first', suggestion: 'Add the price list skill' } },
  ] });
  const executor = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  const job = await app.send(ask);
  await app.done(job);
  assert.ok(JSON.stringify(app.provider.requests).includes('Quotes sent'), 'my_goals returned the goal to the agent');
  const review = (await app.request(`${app.path}/activity/review?days=30&scope=all`, app.cookie)).value;
  assert.equal(review.goals[0].current, 42);
  assert.equal(review.goals[0].risk, 'Two suppliers are late with prices');
  assert.equal(review.reviews[0].suggestion, 'Add the price list skill');
  const weekly = (await app.request(`${app.path}/activity/weekly-review`, app.cookie, { agent: app.profile.id, enabled: true })).value;
  assert.equal(app.sql(`SELECT interval_seconds FROM relay_agent_schedules WHERE id='${weekly.schedule}'`), '604800');
  await app.stop(executor);
  app.pass('Real Goose reads its goal, reports progress with an early risk and writes a weekly review; the manager turns on the weekly routine');
});

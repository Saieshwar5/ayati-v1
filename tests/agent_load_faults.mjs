import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { writeFile } from 'node:fs/promises';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { pinLoadProcess } from './fixtures/load_resources.mjs';
import { actors, requestCase, ready, send, jobs, complete } from './fixtures/load_workload.mjs';
import { until } from './fixtures/goose_app.mjs';

const options = { skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 180000 };

async function setup(t, count) {
  const environment = await pinLoadProcess(t, 4);
  const app = await workerFixture(t, {
    mode: 'pooled', control: true, traceStart: true, resumableHold: true,
    cpuList: environment.cpu_list, capacity: 4, workspaceLimit: 32,
    modelLimit: 32, leaseSeconds: 12,
  });
  const fleet = [await app.startExecutor(), await app.startExecutor()];
  await ready(app);
  const profiles = await actors(app, count, 'small');
  const items = await Promise.all(profiles.map(profile => requestCase(app, profile, 'small')));
  const report = { version: 1, environment, accepted: count, ready_slots: 4, lease_seconds: 12 };
  t.after(() => writeFile(app.data + '/load-fault-report.json', JSON.stringify({ ...report, passed: t.passed }, null, 2)));
  return { app, fleet, items, report };
}

async function drafts(app, items) {
  return until(async () => {
    const rows = await jobs(app, items);
    return rows.length === items.length && rows.every(row => row.progress?.text) && rows;
  }, 'Concurrent drafts did not arrive', 35000);
}

function assertComplete(rows, attempts) {
  for (const row of rows) {
    assert.equal(row.message_state, 'complete');
    assert.equal(row.attempts, attempts);
  }
}

test('controller replacement preserves four running and four queued requests', options, async t => {
  const { app, fleet, items, report } = await setup(t, 8);
  app.provider.setHeld(true);
  await Promise.all(items.map(item => send(app, item)));
  const before = await until(async () => {
    const rows = await jobs(app, items);
    return rows.filter(row => row.state === 'running').length === 4 &&
      app.provider.requests.length === 4 && rows;
  }, 'Four active requests and queued backlog missing', 35000);
  assert.equal(before.filter(row => row.state === 'queued').length, 4);
  const began = Date.now();
  await app.stopApi();
  await app.startApi();
  const restored = await jobs(app, items);
  for (const row of before) {
    const after = restored.find(value => value.reply === row.reply);
    assert.equal(after.claim, row.claim, 'Restart changed durable ownership');
    assert.equal(after.attempts, row.attempts);
  }
  assert.equal(app.provider.requests.length, 4, 'Restart repeated active inference');
  app.provider.setHeld(false);
  const final = await complete(app, items);
  assertComplete(final, 1);
  assert.equal(app.provider.requests.length, 8);
  report.restart_to_all_final_ms = Date.now() - began;
  report.before = before;
  report.final = final;
  for (const executor of fleet) await app.stop(executor);
  app.pass('Controller replacement retains active ownership and backlog; all eight complete with one inference each');
});

test('executor loss retries only its two attempts beside two active survivors', options, async t => {
  const { app, fleet, items, report } = await setup(t, 4);
  app.provider.setStream({ chunks: ['OLD_DRAFT', ' old completion'], holdAfter: 1 });
  app.provider.setOutputHeld(true);
  await Promise.all(items.map(item => send(app, item)));
  const before = await drafts(app, items);
  const pids = await app.gooseChildren(fleet[0]);
  assert.equal(pids.length, 2);
  const identity=await app.executorIdentity(fleet[0]);
  const lostClaims = JSON.parse(await app.sqlAsync(`SELECT COALESCE(json_agg(assignment),'[]') FROM relay_executor_slots WHERE executor='${identity.id}' AND incarnation='${identity.incarnation}' AND state='running'`));
  assert.equal(lostClaims.length, 2);
  const began = Date.now();
  await app.stop(fleet[0], 'SIGKILL');
  await app.assertChildrenStopped(pids);
  app.provider.setStream({ chunks: ['NEW_DRAFT', ' recovered completion'], holdAfter: 1 });
  const replacement = await app.startExecutor();
  const replaced = await until(async () => {
    const rows = await jobs(app, items);
    return rows.length === 4 && rows.every(row => {
      const previous = before.find(old => old.reply === row.reply);
      return lostClaims.includes(previous.claim)
        ? row.attempts === 2 && row.progress?.text === 'NEW_DRAFT'
        : row.attempts === 1 && row.claim === previous.claim && row.progress?.text === 'OLD_DRAFT';
    }) && rows;
  }, 'Lost attempts did not recover independently of survivors', 40000);
  report.loss_to_replacement_draft_ms = Date.now() - began;
  assert.equal(app.provider.requests.length, 6);
  for (const row of replaced.filter(row => row.attempts === 2)) {
    assert.equal(row.progress.generation, 2);
    assert.equal(JSON.stringify(row.progress).includes('OLD_DRAFT'), false);
  }
  app.provider.setOutputHeld(false);
  const final = await complete(app, items);
  for (const row of final) {
    const previous = before.find(old => old.reply === row.reply);
    const lost = lostClaims.includes(previous.claim);
    assert.equal(row.message_state, 'complete');
    assert.equal(row.attempts, lost ? 2 : 1);
    assert.equal(row.content, lost ? 'NEW_DRAFT recovered completion' : 'OLD_DRAFT old completion');
  }
  assert.equal(app.provider.requests.length, 6);
  report.before = before;
  report.final = final;
  await app.stop(replacement);
  await app.stop(fleet[1]);
  app.pass('Actual lease expiry fences the lost executor; only its runs retry and old drafts never join replacement text');
});

test('cancellation and grant revocation fence output beside authorized running requests', options, async t => {
  const { app, fleet, items, report } = await setup(t, 4);
  const memberCookie = await app.login(1);
  const roster = (await app.request('/api/identity/simulation')).value;
  const member = roster.employees.find(employee => employee.number === 1).user.id;
  const revoked = items[1];
  await app.request(app.path + '/chat-grants', app.cookie, { user: member, agent: revoked.agent, allowed: true });
  revoked.chat = (await app.request(app.path + '/chats', app.cookie, {
    id: randomUUID(), name: 'Concurrent revocation', kind: 'channel', users: [member], agents: [revoked.agent],
  })).value.id;
  revoked.cookie = memberCookie;
  app.provider.setStream({ chunks: ['AUTHORIZED_DRAFT', ' FINAL_ONLY_IF_ALLOWED'], holdAfter: 1 });
  app.provider.setOutputHeld(true);
  await Promise.all(items.map(item => send(app, item)));
  const before = await drafts(app, items);
  const cancelled = before.find(row => row.chat === items[0].chat);
  const revokedRow = before.find(row => row.chat === revoked.chat);
  const stoppedClaims = [cancelled.claim, revokedRow.claim];
  const pids = await app.assignmentProcessIds(stoppedClaims);
  await Promise.all([
    app.request(`${app.path}/chats/${cancelled.chat}/messages/${cancelled.reply}/cancel`, app.cookie, {}),
    app.request(app.path + '/chat-grants', app.cookie, { user: member, agent: revoked.agent, allowed: false }),
  ]);
  const redacted = (await app.history(revoked.chat, memberCookie)).find(row => row.id === revokedRow.reply);
  assert.equal(JSON.stringify(redacted).includes('AUTHORIZED_DRAFT'), false, 'Revoked requester can still replay draft');
  await app.assertChildrenStopped(pids, 15000);
  app.provider.setOutputHeld(false);
  const final = await complete(app, items);
  for (const row of final) {
    assert.equal(row.attempts, 1);
    if (stoppedClaims.includes(row.claim)) {
      assert.equal(row.message_state, 'cancelled');
      assert.equal(row.progress, null);
      assert.equal(row.content.includes('FINAL_ONLY_IF_ALLOWED'), false);
    } else {
      assert.equal(row.message_state, 'complete');
      assert.equal(row.content, 'AUTHORIZED_DRAFT FINAL_ONLY_IF_ALLOWED');
    }
  }
  const deniedText = await app.sqlAsync(`SELECT count(*) FROM relay_assignment_output WHERE assignment IN ('${stoppedClaims.join("','")}') AND text LIKE '%FINAL_ONLY_IF_ALLOWED%'`);
  assert.equal(deniedText, '0');
  const replay = (await app.history(revoked.chat, memberCookie)).find(row => row.id === revokedRow.reply);
  assert.equal(JSON.stringify(replay).includes('FINAL_ONLY_IF_ALLOWED'), false);
  assert.equal(app.provider.requests.length, 4);
  report.final = final;
  for (const executor of fleet) await app.stop(executor);
  app.pass('Concurrent Stop and grant revocation clear drafts and stop harnesses; authorized neighbors finish without denied output');
});

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { until } from './fixtures/polling.mjs';

test('Native communication intake, isolated Goose context, restart and uncertain delivery', {
  timeout: 240000, skip: !process.env.RELAY_TEST_DATABASE_URL,
}, async t => {
  const app = await workerFixture(t, { mode: 'pooled', control: true });
  const path = `${app.path}/communications`;
  const sim = `${path}/simulation`;
  const status = () => app.request(path, app.cookie).then(r => r.value);
  const owner = (await app.request('/api/identity/session', app.cookie)).value.user;
  const memberCookie = await app.login(1);
  const member = (await app.request('/api/identity/session', memberCookie)).value.user;
  await app.request(`${app.path}/chat-grants`, app.cookie, { user: member.id, agent: app.profile.id, allowed: true });
  const email = { id: randomUUID(), agent: app.profile.id, transport: 'email', address: 'nova@simulation.invalid' };
  const whatsapp = { id: randomUUID(), agent: app.profile.id, transport: 'whatsapp', address: 'sim-nova-number' };
  for (const endpoint of [email, whatsapp]) {
    await app.request(`${sim}/endpoints`, app.cookie, endpoint);
    await app.request(`${sim}/endpoints/${endpoint.id}/bindings`, app.cookie, { sender: 'sim-owner', user: owner.id });
  }
  await app.request(`${sim}/endpoints/${email.id}/bindings`, app.cookie, { sender: 'sim-member', user: member.id });
  await app.request(path, memberCookie, undefined, 400);
  await app.request(`${sim}/endpoints`, memberCookie, email, 400);
  const event = (id, text, endpoint = email, sender = 'sim-owner') => ({
    event: id, sender, recipient: endpoint.address, conversation: 'thread-1', text,
  });
  const incoming = (input, endpoint = email, expected = 200, cookie = app.cookie) =>
    app.request(`${sim}/endpoints/${endpoint.id}/incoming`, cookie, input, expected);
  const jobFor = id => {
    assert.match(id, /^[a-z0-9-]+$/);
    return JSON.parse(app.sql(`SELECT j.payload FROM relay_chat_jobs j JOIN relay_communication_inbox i ON i.workspace=j.workspace AND i.reply=j.reply WHERE i.event='${id}'`));
  };
  const dispatch = (id, outcome) => app.request(`${sim}/outbox/${id}/dispatch`, app.cookie, { outcome }).then(r => r.value);
  await incoming(event('unauthorized', 'Cannot inject'), email, 400, memberCookie);
  await incoming({ ...event('spoof', 'Cannot choose a principal'), user: owner.id }, email, 422);
  assert.equal((await incoming(event('unknown', 'UNKNOWN_BODY', email, 'unknown'))).value.status, 'held');
  assert.equal((await incoming({ ...event('echo', 'Own echo'), sender: email.address })).value.status, 'ignored');
  assert.equal((await incoming({ ...event('auto', 'Automated reply'), automated: true })).value.status, 'ignored');
  assert.equal(app.sql('SELECT count(*) FROM relay_chat_jobs'), '0');

  const first = event('email-first', 'PRIVATE_EMAIL_PAYROLL_48000');
  const duplicate = await Promise.all([incoming(first), incoming(first)]);
  assert.deepEqual(duplicate.map(r => r.value.duplicate).sort(), [false, true]);
  await incoming({ ...first, text: 'Changed retry' }, email, 400);
  const emailJob = jobFor('email-first');
  await app.stopApi(); await app.startApi();
  assert.equal((await incoming(first)).value.duplicate, true);
  assert.equal(app.sql('SELECT count(*) FROM relay_chat_jobs'), '1');
  let executor = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  await app.done(emailJob);
  assert.equal((await status()).outbox[0].state, 'queued');
  assert.ok(!JSON.stringify(await status()).includes('PRIVATE_EMAIL_PAYROLL_48000'));
  app.pass('HTTP owner boundaries, spoof rejection, loop suppression, held unknown senders, exact dedupe and durable intake across API restart');

  const beforeMember = app.provider.requests.length;
  await incoming(event('member-email', 'Review my own work', email, 'sim-member'));
  const memberJob = jobFor('member-email');
  await until(async () => (await app.history(memberJob.chat, memberCookie)).some(m => m.id === memberJob.reply && m.state === 'complete'), 'Member reply missing');
  assert.notEqual(memberJob.chat, emailJob.chat);
  const memberContext = JSON.stringify(app.provider.requests.slice(beforeMember));
  assert.ok(!memberContext.includes('PRIVATE_EMAIL_PAYROLL_48000'));
  await app.request(`${app.path}/chats/${memberJob.chat}/messages`, app.cookie, undefined, 400);
  const beforeWhatsapp = app.provider.requests.length;
  await incoming(event('whatsapp-first', 'A separate WhatsApp conversation', whatsapp), whatsapp);
  const waJob = jobFor('whatsapp-first');
  await app.done(waJob);
  assert.notEqual(waJob.chat, emailJob.chat);
  assert.ok(!JSON.stringify(app.provider.requests.slice(beforeWhatsapp)).includes('PRIVATE_EMAIL_PAYROLL_48000'));
  const chatJob = await app.send('A separate native Chat conversation');
  await app.done(chatJob);
  assert.equal((await status()).outbox.find(r => r.id === chatJob.reply).state, 'published');
  await app.request(`${app.path}/chats/${emailJob.chat}/messages`, app.cookie, { id: randomUUID(), content: 'Bypass endpoint', agent: app.profile.id }, 400);
  app.pass('Real Goose receives separate employee, transport and Chat contexts; owner metadata does not reveal private bodies');

  const uncertain = await dispatch(emailJob.reply, 'timeout');
  assert.equal(uncertain.state, 'uncertain');
  await app.stop(executor); await app.stopApi(); await app.startApi();
  assert.deepEqual(await dispatch(emailJob.reply, 'delivered'), uncertain);
  const receipt = { attempt: uncertain.attempt, provider_id: `sim:${emailJob.reply}:${uncertain.attempt}`, delivered: true };
  const receiptPath = `${sim}/outbox/${emailJob.reply}/receipt`;
  assert.equal((await app.request(receiptPath, app.cookie, receipt)).value.state, 'delivered');
  assert.equal((await app.request(receiptPath, app.cookie, { ...receipt, delivered: false })).value.state, 'delivered');
  await app.request(receiptPath, app.cookie, { ...receipt, attempt: randomUUID() }, 400);
  assert.equal((await dispatch(waJob.reply, 'rejected')).state, 'failed');
  const crashed = await dispatch(waJob.reply, 'crash');
  assert.equal(crashed.state, 'sending');
  await app.stopApi(); await app.startApi();
  app.sql(`UPDATE relay_communication_outbox SET lease_until=0 WHERE id='${waJob.reply}'`);
  const recovered = await dispatch(waJob.reply, 'delivered');
  assert.equal(recovered.state, 'uncertain'); assert.equal(recovered.attempts, 2);
  app.pass('Uncertain sends survive restart without resend; late receipts cannot downgrade delivery; interrupted dispatch recovers conservatively');

  app.provider.setHeld(true);
  executor = await app.startExecutor({ AYATI_EXECUTOR_SLOTS: '1' });
  const beforeHeld = app.provider.requests.length;
  await incoming(event('revoke-inflight', 'Work before endpoint revocation'));
  const held = jobFor('revoke-inflight');
  await until(() => app.provider.requests.length > beforeHeld, 'Agent did not start');
  await app.request(`${sim}/endpoints/${email.id}/state`, app.cookie, { revision: 1, enabled: false });
  await app.done(held, 'cancelled');
  app.provider.setHeld(false);
  assert.ok(!(await status()).outbox.some(r => r.id === held.reply));
  await app.request(`${sim}/endpoints/${email.id}/state`, app.cookie, { revision: 1, enabled: true }, 400);
  await app.request(`${sim}/endpoints/${email.id}/state`, app.cookie, { revision: 2, enabled: true });
  const beforeNew = app.provider.requests.length;
  await incoming(event('new-generation', 'Use fresh context after reconnect'));
  const fresh = jobFor('new-generation'); await app.done(fresh);
  assert.notEqual(fresh.chat, emailJob.chat);
  assert.ok(!JSON.stringify(app.provider.requests.slice(beforeNew)).includes('PRIVATE_EMAIL_PAYROLL_48000'));
  await app.stop(executor);
  app.pass('Revocation cancels running work and reconnect starts a new context generation');

  // Use the actual normal identity server: no spoofed simulation flag can enable injection.
  await app.stopApi();
  const hosted = app.startHosted({ AYATI_PORT: new URL(app.base).port });
  await until(async () => {
    assert.equal(hosted.child.exitCode, null, hosted.logs);
    try { return (await fetch(`${app.base}/health/ready`)).ok; } catch { return false; }
  }, 'Normal identity server did not start');
  await app.request(`${sim}/endpoints`, '', email, 404);
  await app.stop(hosted);
  app.pass('Simulation mutation API is disabled in the real identity server');
  await writeFile(join(app.data, 'native-communications.json'), JSON.stringify({
    passed: true, real_goose: true, durable_intake: true, isolated_context: true,
    uncertain_delivery: true, restart: true, revocation: true, normal_mode_denial: true,
    live_email: false, live_whatsapp: false,
  }, null, 2));
});

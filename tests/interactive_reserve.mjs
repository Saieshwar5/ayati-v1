import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { writeFile } from 'node:fs/promises';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { pinLoadProcess, sampleResources } from './fixtures/load_resources.mjs';
import { actors, requestCase, ready, send, jobs, complete, samples } from './fixtures/load_workload.mjs';
import { summarize } from './fixtures/load_metrics.mjs';
import { until } from './fixtures/goose_app.mjs';

const options = { skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 240000 };
async function fixture(t, extra = {}) {
  const environment = await pinLoadProcess(t, 4);
  const app = await workerFixture(t, { mode:'pooled', control:true, capacity:4, workspaceLimit:4,
    modelLimit:4, chatReserve:1, leaseSeconds:12, traceStart:true, resumableHold:true,
    cpuList:environment.cpu_list, ...extra });
  app.provider.setStream({chunks:['MIXED_REPLY_OK'],holdAfter:9,firstDelayMs:30});
  return { app, environment };
}
async function deferred(app, item) {
  item.sent_ms = Date.now();
  await app.request(`${app.path}/chats/${item.chat}/background-requests`, item.cookie ?? app.cookie,
    { id:item.id, content:item.content, agent:item.agent });
  item.ack_ms = Date.now();
}
async function running(app, count) {
  return until(async () => Number(await app.sqlAsync("SELECT count(*) FROM relay_chat_jobs WHERE state='running'")) === count,
    'Mixed assignments did not reach the expected active count', 30000);
}

test('chat uses reserved ready capacity beside held background work and backlog', options, async t => {
  const { app, environment } = await fixture(t);
  const report = { version:1, environment, configured_slots:4, chat_reserve:1, cases:[] };
  t.after(() => writeFile(app.data+'/mixed-load-report.json', JSON.stringify({ ...report, passed:t.passed },null,2)));
  const fleet = [await app.startExecutor(), await app.startExecutor()];
  await ready(app);
  const profiles = await actors(app, 4, 'small');
  const background = await Promise.all(profiles.map(profile => requestCase(app,profile,'small')));
  app.provider.setHeldMarkers(background.map(item => item.marker));
  await Promise.all(background.map(item => deferred(app,item)));
  await running(app,3);
  await until(() => app.provider.requests.length===3,'Background calls did not arrive');
  await ready(app,1);
  let rows=await jobs(app,background);
  assert.equal(rows.filter(row=>row.attempts===0&&row.state==='queued').length,1);
  const heldClaims=rows.filter(row=>row.state==='running').map(row=>row.claim);
  const stopResources=await sampleResources([process.pid,app.apiProcess().child.pid,...fleet.map(e=>e.child.pid)]);
  try {
    for (let repeat=1;repeat<=2;repeat++) for (const tier of ['small','large']) {
      const [profile]=await actors(app,1,tier);
      const items=[];
      for (let index=0;index<24;index++) {
        const item=await requestCase(app,profile,tier);
        await ready(app,1); // Reported separately: replenishment is outside ready-capacity timing.
        await send(app,item);
        await complete(app,[item]);
        items.push(item);
        const state=await jobs(app,background);
        assert.deepEqual(state.filter(row=>row.state==='running').map(row=>row.claim).sort(),[...heldClaims].sort(),
          'A held owner was preempted or the reserve admitted more background work');
        assert.equal(state.filter(row=>row.attempts===0).length,1);
      }
      const raw=samples(app,items,await jobs(app,items));
      const summary=summarize(raw);
      report.cases.push({ repeat,tier,raw,summary });
      console.log('MIXED_CASE',JSON.stringify({repeat,tier,...summary}));
    }
  } finally { report.resources=await stopResources(); app.provider.setHeldMarkers([]); }
  rows=await complete(app,background);
  assert.ok(rows.every(row=>row.attempts===1&&row.message_state==='complete'));
  samples(app,background,rows); // Actual scoped Goose calls; no SQL relabeling.
  report.ready_capacity_targets_met=report.cases.every(row=>Object.values(row.summary.provisional_targets).every(Boolean));
  report.background=rows;
  for (const executor of fleet) await app.stop(executor);
  app.pass('96 short chats complete beside three held background runs; reserve and backlog persist, then every background request completes once');
});

test('background intake preserves authority, immutable replay and queue headroom for chat', options, async t => {
  const { app }=await fixture(t);
  const memberCookie=await app.login(1);
  const input={id:randomUUID(),content:'Deferred authorized request',agent:app.profile.id};
  const path=`${app.path}/chats/${app.conversation}`;
  await app.request(path+'/background-requests',memberCookie,input,400);
  await app.request(path+'/background-requests',app.cookie,{...input,work_class:'interactive'},422);
  await app.request(path+'/background-requests',app.cookie,input);
  await app.request(path+'/background-requests',app.cookie,input);
  await app.request(path+'/messages',app.cookie,input,400);
  assert.equal(await app.sqlAsync("SELECT count(*) FROM relay_chat_jobs"),'1');
  for (let i=1;i<32;i++) await app.request(path+'/background-requests',app.cookie,{...input,id:randomUUID()});
  const rejected={...input,id:randomUUID()};
  await app.request(path+'/background-requests',app.cookie,rejected,429);
  assert.equal(await app.sqlAsync(`SELECT count(*) FROM relay_chat_messages WHERE id='${rejected.id}'`),'0');
  await app.request(path+'/messages',app.cookie,{...input,id:randomUUID(),content:'Chat still has queue room'});
  assert.equal(await app.sqlAsync("SELECT count(*) FROM relay_chat_jobs WHERE work_class='interactive'"),'1');
  assert.equal(await app.sqlAsync("SELECT sum(attempts) FROM relay_chat_jobs"),'0');
  app.pass('Background submission uses chat authorization, refuses class spoofing/promotion and exact replay duplicates; queue backpressure leaves interactive headroom');
});

test('a smaller fleet preserves actual reserve and draining lets owned background work finish', options, async t => {
  const { app }=await fixture(t);
  const executor=await app.startExecutor();
  await ready(app,2);
  const profiles=await actors(app,3,'small');
  const items=await Promise.all(profiles.map(profile=>requestCase(app,profile,'small')));
  app.provider.setHeldMarkers(items.map(item=>item.marker));
  await Promise.all(items.map(item=>deferred(app,item)));
  await running(app,1);
  await until(()=>app.provider.requests.length===1,'Held background call missing');
  await ready(app,1);
  const [human]=await actors(app,1,'small');
  const chat=await requestCase(app,human,'small');
  await send(app,chat);await complete(app,[chat]);
  const initial=await jobs(app,items);
  assert.equal(initial.filter(row=>row.attempts===0).length,2,'Configured capacity cannot stand in for live compatible capacity');
  const stopped=app.stop(executor);
  await until(async()=>Number(await app.sqlAsync("SELECT count(*) FROM relay_executors WHERE state='draining'"))===1,'Drain was not registered');
  assert.equal((await jobs(app,items)).filter(row=>row.attempts===0).length,2);
  app.provider.setHeldMarkers([]);
  await stopped;
  const replacement=await app.startExecutor();
  const final=await complete(app,items);
  assert.ok(final.every(row=>row.attempts===1&&row.message_state==='complete'));
  await app.stop(replacement);
  app.pass('One two-slot executor admits only one background request despite a four-run limit; drain preserves ownership and replacement finishes queued work');
});

test('queued background work rechecks requester grants before execution', options, async t => {
  const { app }=await fixture(t);
  const memberCookie=await app.login(1);
  const roster=(await app.request('/api/identity/simulation')).value;
  const member=roster.employees.find(employee=>employee.number===1).user.id;
  await app.request(app.path+'/chat-grants',app.cookie,{user:member,agent:app.profile.id,allowed:true});
  const chat=(await app.request(app.path+'/chats',app.cookie,{id:randomUUID(),name:'Deferred member task',kind:'channel',users:[member],agents:[app.profile.id]})).value.id;
  const item={id:randomUUID(),content:'Deferred before revocation',agent:app.profile.id};
  await app.request(`${app.path}/chats/${chat}/background-requests`,memberCookie,item);
  await app.request(app.path+'/chat-grants',app.cookie,{user:member,agent:app.profile.id,allowed:false});
  const executor=await app.startExecutor();
  await until(async()=>Number(await app.sqlAsync("SELECT count(*) FROM relay_chat_jobs WHERE state='finished'"))===1,'Revoked background request did not finish safely');
  assert.equal(app.provider.requests.length,0,'Revoked background context reached a provider');
  const messages=(await app.request(`${app.path}/chats/${chat}/messages`,memberCookie)).value.messages;
  assert.ok(messages.some(message=>message.kind==='agent'&&['cancelled','failed'].includes(message.state)&&!message.progress));
  await app.stop(executor);
  app.pass('A deferred request is not authorization: grant revocation prevents inference and finishes without a published draft');
});

test('controller restart and executor loss retain background class and fence only lost owners', options, async t => {
  const { app }=await fixture(t);
  const fleet=[await app.startExecutor(),await app.startExecutor()];
  await ready(app);
  const profiles=await actors(app,3,'small');
  const items=await Promise.all(profiles.map(profile=>requestCase(app,profile,'small')));
  app.provider.setHeldMarkers(items.map(item=>item.marker));
  await Promise.all(items.map(item=>deferred(app,item)));
  await running(app,3);
  await until(()=>app.provider.requests.length===3,'Background inferences missing');
  const before=await jobs(app,items);
  await app.stopApi();await app.startApi();
  assert.deepEqual((await jobs(app,items)).map(row=>row.claim).sort(),before.map(row=>row.claim).sort());
  const pids=await app.gooseChildren(fleet[0]);
  const identity=await app.executorIdentity(fleet[0]);
  const claims=JSON.parse(await app.sqlAsync(`SELECT COALESCE(json_agg(assignment),'[]') FROM relay_executor_slots WHERE executor='${identity.id}' AND incarnation='${identity.incarnation}' AND state='running'`));
  assert.ok(claims.length>=1);
  await app.stop(fleet[0],'SIGKILL');
  await app.assertChildrenStopped(pids);
  const replacement=await app.startExecutor();
  await until(async()=>{
    const rows=await jobs(app,items);
    return rows.every(row=>{
      const old=before.find(old=>old.reply===row.reply);
      return claims.includes(old.claim)?row.attempts===2&&row.claim!==old.claim:row.attempts===1&&row.claim===old.claim;
    });
  },'Lost background owners were not recovered independently',40000);
  const classes=JSON.parse(await app.sqlAsync("SELECT json_agg(work_class) FROM relay_chat_jobs"));
  assert.ok(classes.every(value=>value==='background'));
  // Three held background owners still leave capacity for a different agent's chat.
  await ready(app,1);
  const [profile]=await actors(app,1,'small');
  const chat=await requestCase(app,profile,'small');
  await send(app,chat);await complete(app,[chat]);
  app.provider.setHeldMarkers([]);
  const final=await complete(app,items);
  assert.ok(final.every(row=>row.message_state==='complete'));
  await app.stop(replacement);await app.stop(fleet[1]);
  app.pass('API replacement preserves owners/classes; actual executor loss retries only its expired attempts and chat retains reserved capacity');
});

// Living panels with real Goose (people-and-agents-first, Phase 3): the agent's
// update saves bindings; the next update is run by the controller with no model
// call; a meaningful move goes back to the agent, who writes what changed; and the
// agent's own work on the source updates the panel. The model is scripted; Goose,
// the controller, ayati_control and a remote MCP server are real.
import {test} from 'node:test';
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {mkdtemp,writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {workerFixture} from './fixtures/independent_worker.mjs';
import {remoteControlMcp} from './fixtures/remote_control_mcp.mjs';
import {until,root} from './fixtures/polling.mjs';

test('a living panel refreshes from the source without the model, asks the agent on a real change, and follows its work',{
  skip:!process.env.RELAY_TEST_DATABASE_URL,timeout:240000,
},async t=>{
  const remote=await remoteControlMcp();t.after(()=>remote.close());
  const dir=await mkdtemp(join(root,'.tmp/living-panels-profile-'));
  const profiles=join(dir,'servers.json'),bearer=join(dir,'bearer');
  await writeFile(profiles,'[]',{mode:0o600});await writeFile(bearer,remote.token,{mode:0o600});
  const app=await workerFixture(t,{mode:'pooled',control:true,capacity:1,workspaceLimit:1,modelLimit:1,leaseSeconds:15,
    apiEnv:{AYATI_MCP_SERVERS_FILE:profiles}});
  const w=app.path.split('/').at(-1),member=await app.login(1);
  const person=(await app.request('/api/identity/session',member)).value.user;
  await app.request(`${app.path}/chat-grants`,app.cookie,{user:person.id,agent:app.profile.id,allowed:true});
  const channel=(await app.request(`${app.path}/chats`,app.cookie,{id:randomUUID(),kind:'channel',name:'Order desk',users:[person.id],agents:[app.profile.id]})).value.id;
  const connection=randomUUID(),control=`${app.path}/agent-profiles/${app.profile.id}/control`;
  await writeFile(profiles,JSON.stringify([{id:'orders',workspace:w,name:'Orders',account:'orders@example.test',url:remote.url,bearer_file:bearer,operations:remote.operations}]));
  await app.request(`${app.path}/control/systems`,app.cookie,{server:'orders',approved:true,revision:0});
  await app.request(`${app.path}/control/connections`,app.cookie,{id:connection,server:'orders',enabled:true,revision:0});
  await app.request(`${app.path}/control/connector-rules`,app.cookie,{connection,operation:'order.read',mode:'allow',needs_other:false,revision:0});
  await app.request(`${control}/tools`,app.cookie,{connection,enabled:true});
  remote.records.set('one',{order:'one',address:'Main road',items:3});
  const read={tool:'request_action',arguments:{connection,operation:'order.read',arguments:{order:'one'}}};
  const id=randomUUID(),prompt='Make me a panel with the item count of order one.';
  app.provider.setControl(prompt,{steps:[read,{tool:'save_home_panel',arguments:{id,expected_revision:0,title:'Order one items',purpose:'Items in order one',
    template:'metric',sections:[{kind:'metric',label:'Items',value:'3'}],sources:[]}}],checkpoint:'Saved a draft.'});
  const manual='Refresh my personal Home panel using home_panel_update_context, its reviewed reads and refresh_home_panel.';
  const update=(value,extra={})=>app.provider.setControl(manual,{steps:[{tool:'home_panel_update_context',arguments:{}},read,
    {tool:'refresh_home_panel',arguments:{sections:[{kind:'metric',label:'Items',value}],...extra}}],checkpoint:`Updated: ${value}`});
  const executor=await app.startExecutor({AYATI_EXECUTOR_SLOTS:'1'});
  await app.done(await app.send(prompt,channel,member));
  const panels=`${app.path}/home-panels`;
  await app.request(`${panels}/${id}/review`,member,{request:randomUUID(),revision:1});
  await app.request(`${panels}/${id}/maintenance`,member,{request:randomUUID(),revision:0,panel_revision:1,enabled:true,
    instructions:'Read order one and show its item count.',interval_seconds:null,stale_seconds:3600,min_gap_seconds:60});
  const now=async()=>{const m=(await app.request(panels,member)).value.panels[0].maintenance;
    return (await app.request(`${panels}/${id}/refresh`,member,{request:randomUUID(),revision:m.revision})).value.reply;};
  const run=reply=>JSON.parse(app.sql(`SELECT to_jsonb(r) FROM relay_home_update_runs r WHERE reply='${reply}'`));
  const finished=reply=>until(()=>app.sql(`SELECT state FROM relay_chat_jobs WHERE reply='${reply}'`)==='finished','Update did not finish',40000);
  const shown=async()=>(await app.request(panels,member)).value.panels[0].active;

  // 1. The agent's update proposes bindings; they reproduce it, so they are kept.
  update('3',{bindings:[{target:'value',read:0,path:'items'}]});
  const first=await now();await finished(first);
  assert.equal(JSON.parse(app.sql(`SELECT bindings FROM relay_home_maintenance WHERE panel='${id}'`))[0].path,'items');

  // 2. The next update is the controller's: the source is read, no model is called.
  const calls=remote.calls.length,model=app.provider.requests.length;
  const second=await now();await finished(second);
  assert.equal(run(second).direct,true);
  assert.equal(app.provider.requests.length,model,'no model call');
  assert.equal(remote.calls.length,calls+1,'one real source read');
  assert.equal((await shown()).from_source,true);
  assert.equal(app.sql(`SELECT content FROM relay_chat_messages WHERE id='${second}'`),'Numbers refreshed from the source.');

  // 3. A meaningful move goes to the agent, who writes what changed.
  remote.records.set('one',{order:'one',address:'Main road',items:9});
  update('9',{what_changed:'Order one went from 3 to 9 items.'});
  const third=await now();await finished(third);
  assert.match(run(third).fallback,/meaningful/);
  assert.ok(app.provider.requests.length>model,'the agent was asked');
  const after=await shown();
  assert.equal(after.definition.sections[0].value,'9');assert.equal(after.what_changed,'Order one went from 3 to 9 items.');

  // 4. The agent's own work on the source updates the panel (after the gap).
  const ask='Check order one again for me.';
  app.provider.setControl(ask,{steps:[read],checkpoint:'Order one has 9 items.'});
  await app.done(await app.send(ask,channel,member));
  await until(()=>app.sql(`SELECT count(*) FROM relay_home_update_wanted WHERE panel='${id}'`)==='1','Work did not ask for an update');
  app.sql(`UPDATE relay_home_update_runs SET created_ms=created_ms-120000 WHERE panel='${id}'`);
  app.sql(`UPDATE relay_home_update_wanted SET due_ms=1 WHERE panel='${id}'`);
  await until(()=>app.sql(`SELECT count(*) FROM relay_home_update_runs WHERE panel='${id}' AND reason='work'`)==='1','Work did not update the panel',40000);
  const fourth=app.sql(`SELECT reply FROM relay_home_update_runs WHERE panel='${id}' AND reason='work'`);
  await finished(fourth);
  assert.equal(run(fourth).direct,true,'nothing meaningful moved: the controller did it');
  const history=(await app.request(`${app.path}/activity/history?kind=panel&scope=all`,member)).value.items;
  assert.ok(history.some(item=>item.summary==="Updated panel: Order one items · after the agent's work"));
  await app.stop(executor);
  app.pass('Bindings kept from a real update; the controller refreshes from the real source with no model call; a meaningful change goes to the agent; the agent\'s work updates the panel');
});

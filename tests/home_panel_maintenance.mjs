import {test} from 'node:test';
import assert from 'node:assert/strict';
import {randomUUID} from 'node:crypto';
import {createRequire} from 'node:module';
import {mkdtemp,writeFile} from 'node:fs/promises';
import {join} from 'node:path';
import {workerFixture} from './fixtures/independent_worker.mjs';
import {remoteControlMcp} from './fixtures/remote_control_mcp.mjs';
import {until,root} from './fixtures/polling.mjs';
const {chromium}=createRequire(join(root,'web/package.json'))('@playwright/test');

test('personal panel agreements drive real managed reads, automatic snapshots, schedules and restart-safe Home',{
  skip:!process.env.RELAY_TEST_DATABASE_URL,timeout:240000,
},async t=>{
  const remote=await remoteControlMcp();t.after(()=>remote.close());
  const dir=await mkdtemp(join(root,'.tmp/home-maintenance-profile-'));
  const profiles=join(dir,'servers.json'),bearer=join(dir,'bearer');
  await writeFile(profiles,'[]',{mode:0o600});await writeFile(bearer,remote.token,{mode:0o600});
  const app=await workerFixture(t,{mode:'pooled',control:true,capacity:1,workspaceLimit:1,modelLimit:1,leaseSeconds:15,
    apiEnv:{AYATI_MCP_SERVERS_FILE:profiles}});
  const w=app.path.split('/').at(-1),member=await app.login(1);
  const person=(await app.request('/api/identity/session',member)).value.user;
  await app.request(`${app.path}/chat-grants`,app.cookie,{user:person.id,agent:app.profile.id,allowed:true});
  const channel=(await app.request(`${app.path}/chats`,app.cookie,{id:randomUUID(),kind:'channel',name:'Panel source work',users:[person.id],agents:[app.profile.id]})).value.id;
  const connection=randomUUID(),control=`${app.path}/agent-profiles/${app.profile.id}/control`;
  await writeFile(profiles,JSON.stringify([{id:'orders',workspace:w,name:'Orders',account:'orders@example.test',url:remote.url,bearer_file:bearer,operations:remote.operations}]));
  await app.request(`${app.path}/control/systems`,app.cookie,{server:'orders',approved:true,revision:0});
  await app.request(`${app.path}/control/connections`,app.cookie,{id:connection,server:'orders',enabled:true,revision:0});
  for(const operation of ['order.read','order.update'])await app.request(`${control}/rules`,app.cookie,{connection,operation,mode:'allow',revision:0,chat:channel});
  const id=randomUUID(),prompt='Prepare my order address panel.';
  app.provider.setControl(prompt,{steps:[
    {tool:'request_action',arguments:{connection,operation:'order.read',arguments:{order:'one'}}},
    {tool:'save_home_panel',arguments:{id,expected_revision:0,title:'My order address',purpose:'Track the reviewed order',template:'metric',
      sections:[{kind:'metric',label:'Address',value:'Old address'}],sources:[]}},
  ],checkpoint:'Saved a personal draft for your review.'});
  const manual='Refresh my personal Home panel using home_panel_update_context, its reviewed reads and refresh_home_panel.';
  const scheduled='Refresh the controller-bound personal Home panel. Use home_panel_update_context, perform its reviewed reads, then refresh_home_panel.';
  function maintenance(command,value){app.provider.setControl(command,{steps:[
    {tool:'home_panel_update_context',arguments:{}},
    {tool:'request_action',arguments:{connection,operation:'order.read',arguments:{order:'one'}}},
    {tool:'refresh_home_panel',arguments:{sections:[{kind:'metric',label:'Address',value}]}},
  ],checkpoint:`Personal maintenance result: ${value}`});}
  maintenance(manual,'Updated address');maintenance(scheduled,'Scheduled address');
  let executor=await app.startExecutor({AYATI_EXECUTOR_SLOTS:'1'});
  const original=await app.send(prompt,channel,member);await app.done(original);
  const browser=await chromium.launch({headless:true});let page;
  t.after(async()=>{if(!t.passed&&page){await page.screenshot({path:join(app.data,'maintenance-failure.png')}).catch(()=>{});
    await writeFile(join(app.data,'maintenance-failure.html'),await page.content()).catch(()=>{});}await browser.close();});
  async function pageFor(cookie){const context=await browser.newContext({viewport:{width:1440,height:960}});
    const [name,value]=cookie.split('=');await context.addCookies([{name,value,url:app.base,httpOnly:true,sameSite:'Lax'}]);return context.newPage();}
  page=await pageFor(member);const otherDevice=await pageFor(member),ownerPage=await pageFor(app.cookie);
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  const home=`${app.base}/w/${w}/?view=home`,card=()=>page.locator(`[data-panel="${id}"]`);
  await page.goto(home);await card().getByRole('button',{name:'Review draft 1',exact:true}).click();
  await page.getByRole('dialog',{name:'Review Home panel',exact:true}).getByRole('button',{name:'Keep on Home',exact:true}).click();
  await until(async()=>(await app.request(`${app.path}/home-panels`,member)).value.panels[0]?.active_revision===1,'Panel review not saved');
  const before=app.provider.requests.length;
  await card().getByRole('button',{name:'Panel updates',exact:true}).click();
  let dialog=page.getByRole('dialog',{name:'Panel updates',exact:true});
  await dialog.getByLabel('What should the agent check?',{exact:true}).fill('Read this order and update its address.');
  await dialog.getByLabel('Check every (minutes)',{exact:true}).fill('1');
  await dialog.getByLabel('Consider delayed after (minutes)',{exact:true}).fill('2');
  await page.screenshot({path:join(app.data,'maintenance-agreement-desktop.png')});
  assert.equal(app.provider.requests.length,before);
  await dialog.getByRole('button',{name:'Enable updates',exact:true}).click();await dialog.waitFor({state:'detached'});
  remote.records.set('one',{order:'one',address:'Updated address'});
  await card().getByRole('button',{name:'Update now',exact:true}).click();
  await until(()=>app.sql(`SELECT count(*) FROM relay_home_snapshots WHERE panel='${id}'`) === '1','Manual update did not publish',40000);
  await card().getByText('Updated address',{exact:true}).waitFor();
  await until(async()=>(await app.request(`${app.path}/home-panels`,member)).value.panels[0]?.maintenance.status==='current','Manual update did not finish');
  assert.equal(remote.calls.length,2);assert.ok(remote.calls.every(c=>c.name==='read_order'));
  await otherDevice.goto(home);await otherDevice.locator(`[data-panel="${id}"]`).getByText('Updated address',{exact:true}).waitFor();
  await ownerPage.goto(home);assert.equal(await ownerPage.locator(`[data-panel="${id}"]`).count(),0);
  const ownerHistory=(await app.request(`${app.path}/chats/${channel}/messages`,app.cookie)).value;
  assert.ok(!JSON.stringify(ownerHistory).includes('Personal maintenance result: Updated address'),'A shared source must not expose personal maintenance output to another Owner');
  await app.stop(executor);await app.stopApi();await app.startApi();
  executor=await app.startExecutor({AYATI_EXECUTOR_SLOTS:'1'});
  await page.reload();await card().getByText('Updated address',{exact:true}).waitFor();
  remote.records.set('one',{order:'one',address:'Scheduled address'});
  await card().getByRole('button',{name:'Panel updates',exact:true}).click();dialog=page.getByRole('dialog',{name:'Panel updates',exact:true});
  await dialog.getByLabel('What should the agent check?',{exact:true}).fill('UNSAVED_UPDATE_INSTRUCTIONS');
  await until(()=>Number(app.sql(`SELECT generation FROM relay_home_maintenance WHERE panel='${id}'`))>=2,'Scheduled update did not publish after controller restart',90000);
  await card().getByText('Scheduled address',{exact:true}).waitFor();
  assert.equal(await dialog.getByLabel('What should the agent check?',{exact:true}).inputValue(),'UNSAVED_UPDATE_INSTRUCTIONS','Routine snapshots must preserve an open agreement draft');
  await dialog.getByRole('button',{name:'Pause updates',exact:true}).click();await dialog.waitFor({state:'detached'});
  await card().getByText(/Updates paused/).waitFor();
  for(const [width,height,dark]of [[1440,960,false],[390,844,false],[320,640,true]]){
    await page.setViewportSize({width,height});await page.evaluate(dark=>{document.documentElement.dataset.theme=dark?'dark':'light';},dark);
    await card().scrollIntoViewIfNeeded();
    assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
    const metric=await card().evaluate(node=>{
      const content=node.querySelector('.panel-snapshot').getBoundingClientRect();
      const value=node.querySelector('.panel-metric-value').getBoundingClientRect();
      return {visible:value.top>=content.top && value.bottom<=content.bottom+1};
    });assert.ok(metric.visible,'The full metric must remain visible above its update controls');
    await page.screenshot({path:join(app.data,`maintenance-${width}-${dark?'dark':'light'}.png`)});
    await card().getByRole('button',{name:'Panel updates',exact:true}).click();
    dialog=page.getByRole('dialog',{name:'Panel updates',exact:true});await dialog.getByRole('button',{name:'Close',exact:true}).focus();
    await page.keyboard.press('Escape');await dialog.waitFor({state:'detached'});
    assert.equal(await card().getByRole('button',{name:'Panel updates',exact:true}).evaluate(n=>n===document.activeElement),true);
  }
  const idle=app.provider.requests.length;await page.reload();await card().getByText('Scheduled address',{exact:true}).waitFor();
  assert.equal(app.provider.requests.length,idle,'Home reads must not wake an agent');
  await app.request(`${control}/rules`,app.cookie,{connection,operation:'order.read',mode:'block',revision:1,chat:channel});
  await card().getByText('Saved content is hidden because its source access changed.',{exact:true}).waitFor();
  assert.ok(!(await page.content()).includes('Scheduled address'));
  assert.ok(remote.calls.every(c=>c.name==='read_order'));assert.deepEqual(errors,[]);
  await writeFile(join(app.data,'home-maintenance-result.json'),JSON.stringify({passed:true,reads:remote.calls.length,externalWrites:0,
    modelRequests:app.provider.requests.length,privateOwnerDenial:true,restart:true,scheduled:true,manual:true},null,2));
  app.pass('Personal agreement, real Goose manual/scheduled refresh, source privacy, second device, restart, pause, phone/dark and source revocation');
});

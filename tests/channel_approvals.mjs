import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { remoteControlMcp } from './fixtures/remote_control_mcp.mjs';
import { until } from './fixtures/polling.mjs';
const root=resolve(import.meta.dirname,'..');
const {chromium}=createRequire(join(root,'web/package.json'))('playwright');

test('Ask first reaches every manager, someone else approves, and conversation membership alone decides nothing',{skip:!process.env.RELAY_TEST_DATABASE_URL,timeout:180000},async t=>{
  const remote=await remoteControlMcp();t.after(()=>remote.close());
  const directory=await mkdtemp(join(root,'.tmp/channel-profile-'));
  const profiles=join(directory,'servers.json'),bearer=join(directory,'bearer');
  await writeFile(profiles,'[]',{mode:0o600});await writeFile(bearer,remote.token,{mode:0o600});
  const app=await workerFixture(t,{mode:'pooled',control:true,runSeconds:90,leaseSeconds:15,apiEnv:{AYATI_MCP_SERVERS_FILE:profiles}});
  const w=app.path.split('/').at(-1),connection=randomUUID();
  await writeFile(profiles,JSON.stringify([{id:'orders',workspace:w,name:'Company orders',account:'Northstar test store',url:remote.url,bearer_file:bearer,operations:remote.operations}]));
  const creator=await app.login(1),approver=await app.login(2),observer=await app.login(3);
  const who=async cookie=>(await app.request('/api/identity/session',cookie)).value.user;
  const [ownerUser,creatorUser,approverUser,observerUser]=await Promise.all([who(app.cookie),who(creator),who(approver),who(observer)]);
  await app.request(`${app.path}/control/systems`,app.cookie,{server:'orders',approved:true,revision:0});
  await app.request(`${app.path}/control/connections`,app.cookie,{id:connection,server:'orders',enabled:true,revision:0});
  // The admin's rule: changing an order asks first, and someone other than the requester approves.
  await app.request(`${app.path}/control/connector-rules`,app.cookie,{connection,operation:'order.update',mode:'ask',needs_other:true,revision:0});
  // A member creates the agent, names a second manager and switches the connector on.
  const agent={id:randomUUID(),name:'Order support',role:'Order support',model:'fixture',audience:'everyone',managers:[creatorUser.id,approverUser.id],revision:0,settings:{provider:'openai',instructions:'Use assigned order operations and request exact approval before changes.'}};
  await app.request(`${app.path}/agent-profiles`,creator,agent);
  const control=`${app.path}/agent-profiles/${agent.id}/control`;
  await app.request(`${control}/tools`,creator,{connection,enabled:true});
  const source=(await app.request(`${app.path}/chats`,creator,{id:randomUUID(),name:'Order work',kind:'group',users:[ownerUser.id,approverUser.id,observerUser.id],agents:[agent.id]})).value.id;
  const browser=await chromium.launch({headless:true});const pages=[];const errors=[];
  t.after(async()=>{if(!t.passed)for(const [i,page]of pages.entries()){await page.screenshot({path:join(app.data,`failure-${i}.png`)}).catch(()=>{});await writeFile(join(app.data,`failure-${i}.html`),await page.content()).catch(()=>{});}await browser.close();});
  async function pageFor(cookie){const context=await browser.newContext({viewport:{width:1280,height:900}});const[name,value]=cookie.split('=');await context.addCookies([{name,value,url:app.base,httpOnly:true,sameSite:'Lax'}]);const page=await context.newPage();page.on('pageerror',e=>errors.push(e.message));pages.push(page);return page;}
  const approverPage=await pageFor(approver),observerPage=await pageFor(observer);
  const snapshot=async()=>(await app.request(control,creator)).value;
  const worker=await app.startExecutor({AYATI_EXECUTOR_SLOTS:'1'});
  async function propose(address){const command=`control ${JSON.stringify({tool:'request_action',arguments:{connection,operation:'order.update',arguments:{order:'one',address}}})}`;const job=await app.send(command,source,creator,agent.id);await until(async()=>(await app.history(source,creator)).find(m=>m.id===job.reply&&(m.state==='complete'||m.progress?.state==='approval')),'Agent did not finish',35000);return (await snapshot()).actions.find(a=>a.job===job.reply);}
  const action=await propose('PRIVATE_REVIEWED_ADDRESS');assert.equal(action.state,'pending');
  // An admin who isn't a manager waits for the backup delay; the requester can't approve.
  await app.request(`${control}/actions/${action.id}/decision`,app.cookie,{id:randomUUID(),decision:'approve'},400);
  await app.request(`${control}/actions/${action.id}/decision`,creator,{id:randomUUID(),decision:'approve'},400);
  const cards=async cookie=>(await app.request(`${app.path}/chats/${source}/messages`,cookie)).value.messages.filter(m=>m.approval);
  const firstCards=await cards(approver);assert.equal(firstCards.length,1);assert.equal(firstCards[0].approval.id,action.id);assert.equal(JSON.stringify(firstCards).includes('PRIVATE_REVIEWED_ADDRESS'),false);
  await approverPage.goto(`${app.base}/w/${w}/?view=chat&conversation=${source}`);
  await approverPage.getByRole('button',{name:'Review saved request',exact:true}).click();
  const review=approverPage.getByRole('dialog',{name:'Review approval request'});
  await review.getByRole('button',{name:'Approve this action',exact:true}).waitFor();
  const text=await review.innerText();assert.match(text,/PRIVATE_REVIEWED_ADDRESS/);assert.match(text,/Someone other than the person who asked must approve/);
  await approverPage.screenshot({path:join(app.data,'manager-approval-desktop.png')});
  await approverPage.setViewportSize({width:390,height:844});await approverPage.screenshot({path:join(app.data,'manager-approval-mobile.png')});assert.equal(await approverPage.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);await approverPage.setViewportSize({width:1280,height:900});
  // Someone else in the conversation sees the request but cannot decide it. (Changing
  // its members while work waits would fence that work, so they joined at the start.)
  assert.equal((await app.request(`${app.path}/control/actions/${action.id}`,observer)).value.can_decide,false);
  await app.request(`${control}/actions/${action.id}/decision`,observer,{id:randomUUID(),decision:'approve'},400);
  await observerPage.goto(`${app.base}/w/${w}/?view=chat&conversation=${source}`);await observerPage.getByRole('button',{name:'Review saved request',exact:true}).click();
  assert.equal(await observerPage.getByRole('button',{name:'Approve this action'}).count(),0);
  await review.getByRole('button',{name:'Approve this action',exact:true}).click();await until(async()=>remote.records.get('one').address==='PRIVATE_REVIEWED_ADDRESS','Approved external write missing');
  assert.equal(remote.calls.filter(c=>c.name==='update_order').length,1);
  await app.request(`${control}/actions/${action.id}/decision`,approver,{id:randomUUID(),decision:'approve'});assert.equal(remote.calls.length,1);
  app.pass('Ask first reaches the agent\'s other manager; the requester, an admin before the backup delay and a conversation member cannot approve; one exact write runs');

  const revoked=await propose('MUST_NOT_EXECUTE');
  await app.request(`${app.path}/control/connector-rules`,app.cookie,{connection,operation:'order.update',mode:'block',needs_other:true,revision:(await app.request(`${app.path}/control/connections`,app.cookie)).value.safety[0].operations.find(o=>o.id==='order.update').revision});
  await until(async()=>(await snapshot()).actions.find(a=>a.id===revoked.id)?.state==='invalidated','Revocation did not fence action');
  await app.request(`${control}/actions/${revoked.id}/decision`,approver,{id:randomUUID(),decision:'approve'},400);assert.equal(remote.calls.length,1);
  await app.request(`${app.path}/control/connector-rules`,app.cookie,{connection,operation:'order.update',mode:'ask',needs_other:true,revision:(await app.request(`${app.path}/control/connections`,app.cookie)).value.safety[0].operations.find(o=>o.id==='order.update').revision});
  const changes=await propose('NEEDS_REVISION');
  await review.getByRole('button',{name:'Close',exact:true}).click();await approverPage.goto(`${app.base}/w/${w}/?view=home`);await approverPage.getByRole('button',{name:/^View attention/}).click();
  const inbox=approverPage.getByRole('dialog',{name:'Your attention',exact:true});await inbox.locator('article').filter({hasText:'Update order'}).getByRole('button',{name:'Review action',exact:true}).click();
  await review.getByRole('button',{name:'Request changes',exact:true}).click();await review.getByRole('status').filter({hasText:'Changes requested'}).waitFor();assert.equal(remote.calls.length,1);
  assert.equal((await snapshot()).actions.find(a=>a.id===changes.id).decision,'request_changes');
  await approverPage.evaluate(()=>{document.documentElement.dataset.theme='dark';});await approverPage.screenshot({path:join(app.data,'manager-attention-dark.png')});
  await app.request(`${app.path}/chats/${source}/members`,creator,{users:[creatorUser.id,ownerUser.id]});
  await until(async()=>await review.locator('dl').count()===0,'Revoked source details remained visible',10000);
  await app.stopApi();await app.startApi();assert.equal((await cards(creator))[0].approval.id,action.id);
  assert.equal(remote.records.get('one').address,'PRIVATE_REVIEWED_ADDRESS');assert.deepEqual(errors,[]);assert.equal(app.processes().some(p=>p.logs.includes(remote.token)),false);assert.equal(JSON.stringify(app.provider.requests).includes(remote.token),false);
  await app.stop(worker);app.pass('Revocation and request changes prevent writes; Home attention reaches the manager; losing the conversation clears open details and receipts survive restart');
});

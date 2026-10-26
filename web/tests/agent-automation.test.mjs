import { registerHooks } from 'node:module';
registerHooks({load(url,context,next){return url.endsWith('.css')?{format:'module',source:'export {};',shortCircuit:true}:next(url,context);}});
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { reactFixture } from './react-fixture.mjs';
const w='11111111-1111-4111-8111-111111111111',url=`http://localhost/w/${w}/?view=agents`;
const session={user:{id:'owner',email:'owner@example.test'},workspaces:[{id:w,name:'Store',role:'Owner'}]};
const agent={id:'agent',name:'Nova',role:'Help customers',model:'fixture',scope:'workspace',revision:3,editable:true,settings:{}};
const clock={location:'Business time',timezone:'Asia/Kolkata',hour12:true};
const routine={id:'routine',chat:'channel',name:'PRIVATE_DELIVERY_REVIEW',content:'PRIVATE_TASK_INSTRUCTIONS',first_ms:Date.now()+3600000,interval_seconds:3600,timezone:'Asia/Kolkata',principal:'owner',principal_name:'Ananya',chat_name:'PRIVATE_DELIVERY_CHANNEL',revision:2,state:'enabled',next_ms:Date.now()+3600000,can_resume:true,detail:''};
const directory={schedules:[routine],conversations:[{id:'channel',name:'PRIVATE_DELIVERY_CHANNEL',kind:'channel'}],more_conversations:false,execution_available:true};
const response=(data,status=200)=>({ok:status<400,status,json:async()=>data});
async function setup(){const context=await import('../src/workspace-context.ts');await context.enterWorkspace();return import('../src/react/agents/automation/create-dialog.tsx');}
function base(path){if(path.includes('/event-triggers'))return response({triggers:[],choices:[],execution_available:true});if(path.endsWith('/session'))return response(session);if(path.includes('/home/preferences'))return response({preferences:clock});if(path.endsWith('/runs'))return response({runs:[]});if(path.includes('/schedules/'))return response(routine);return response(directory);}
const field=label=>[...document.querySelectorAll('label')].find(node=>node.textContent.startsWith(label))?.querySelector('input,textarea,select');
async function fill(change){await change(field('Routine name'),'Delivery review');await change(field('Task instructions'),'Prepare the delivery summary. Ask separately before changing an order.');}

test('schedule context review starts no work and concurrent explicit saves retain one exact payload',async()=>{
 const writes=[];let finish;
 await reactFixture(async({render,button,change,click,update})=>{
  const {CreateSchedule}=await setup();await render(CreateSchedule,{agent:'agent',data:directory,clock,close:()=>{},saved:async()=>{}});
  await fill(change);await click(button('Review schedule'));assert.equal(writes.length,0);assert.match(document.body.textContent,/PRIVATE_DELIVERY_CHANNEL/);assert.match(document.body.textContent,/grants no new permissions/);
  await update(()=>{button('Create schedule').click();button('Create schedule').click();});assert.equal(writes.length,1);assert.equal(writes[0].chat,'channel');assert.equal(writes[0].timezone,'Asia/Kolkata');assert.equal(writes[0].interval_seconds,null);
  await update(()=>finish(response(routine)));assert.match(document.body.textContent,/Schedule saved/);
 },{url,fetch:async(path,init={})=>{if(init.method==='POST'){writes.push(JSON.parse(init.body));return new Promise(resolve=>{finish=resolve;});}return base(path);}});
});

test('unacknowledged routine saves keep their exact ID across reopen and editing explicitly creates another request',async()=>{
 const writes=[];
 await reactFixture(async({render,button,change,click})=>{
  const {CreateSchedule}=await setup();const props={agent:'agent',data:directory,clock,close:()=>{},saved:async()=>{}};
  await render(CreateSchedule,props);await fill(change);await click(button('Review schedule'));await click(button('Create schedule'));assert.match(document.body.textContent,/Save result unavailable/);
  await render(()=>createElement('p',null,'Closed'));await render(CreateSchedule,props);assert.match(document.body.textContent,/A save was not acknowledged/);
  await click(button('Retry same save'));assert.deepEqual(writes[1],writes[0]);
  await click(button('Edit as a new schedule'));assert.equal(field('Routine name').value,'Delivery review');assert.equal(field('Task instructions').value,writes[0].content);
  await change(field('Routine name'),'Different explicit routine');await click(button('Review schedule'));await click(button('Create schedule'));assert.notEqual(writes[2].id,writes[0].id);
 },{url,fetch:async(path,init={})=>{if(init.method==='POST'){writes.push(JSON.parse(init.body));return response({error:'Save result unavailable'},503);}return base(path);}});
});

test('ambiguous or missing local times and unsupported intervals cannot reach the schedule save boundary',async()=>{
 const writes=[];
 await reactFixture(async({render,button,change,click})=>{
  const {CreateSchedule}=await setup();await render(CreateSchedule,{agent:'agent',data:directory,clock:{...clock,timezone:'America/New_York'},close:()=>{},saved:async()=>{}});
  await fill(change);await change(field('First run'),'2026-11-01T01:30');await click(button('Review schedule'));assert.match(document.body.textContent,/missing or repeated/);assert.equal(button('Create schedule'),undefined);
  await change(field('First run'),'2026-11-02T09:00');await change(field('Repeat'),'interval');await change(field('Every'),'0');await click(button('Review schedule'));assert.match(document.body.textContent,/at most 365/);assert.deepEqual(writes,[]);
 },{url,fetch:async(path,init={})=>{if(init.method==='POST')writes.push(path);return base(path);}});
});

test('pause and resume are explicit versioned changes while run reading and role denial start no execution',async()=>{
 const writes=[];
 await reactFixture(async({render,button,click,update})=>{
  await setup();const {ScheduleState}=await import('../src/react/agents/automation/state-dialog.tsx');
  await render(ScheduleState,{agent:'agent',routine,enabled:false,close:()=>{},saved:async()=>{}});assert.equal(writes.length,0);
  await update(()=>{button('Pause routine').click();button('Pause routine').click();});assert.equal(writes.length,1);assert.equal(writes[0].revision,2);assert.equal(writes[0].enabled,false);
  const {AutomationPane}=await import('../src/react/agents/automation/pane.tsx');await render(AutomationPane,{agent:{...agent,editable:false}});assert.match(document.body.textContent,/Only this agent's maintainers/);assert.equal(button('Create schedule'),undefined);assert.equal(writes.length,1);
  await render(AutomationPane,{agent});await click(button('View runs'));assert.match(document.body.textContent,/No occurrences/);assert.equal(writes.length,1);
 },{url,fetch:async(path,init={})=>{if(init.method==='POST'){writes.push(JSON.parse(init.body));return response({id:'routine',revision:3,state:'paused'});}return base(path);}});
});

test('a late creation acknowledgment after unmount cannot refresh or reopen the previous agent',async()=>{
 let finish,refreshes=0;
 await reactFixture(async({render,button,change,click,update})=>{
  const {CreateSchedule}=await setup();await render(CreateSchedule,{agent:'agent',data:directory,clock,close:()=>{},saved:async()=>{refreshes++;}});
  await fill(change);await click(button('Review schedule'));await click(button('Create schedule'));await render(()=>createElement('p',null,'Other agent'));
  await update(()=>finish(response(routine)));assert.equal(refreshes,0);assert.equal(document.querySelector('dialog'),null);
 },{url,fetch:async(path,init={})=>init.method==='POST'?new Promise(resolve=>{finish=resolve;}):base(path)});
});

test('source invalidation immediately clears private routine/run details before delayed revalidation',async()=>{
 let stream,denied=false,stopLive;const deferred=[];const original=globalThis.EventSource;
 try {
  globalThis.EventSource=class{constructor(){this.listeners=new Map();stream=this;}addEventListener(name,fn){this.listeners.set(name,fn);}close(){}};
  await reactFixture(async({render,button,click,update})=>{
   try {
    await setup();const {AutomationPane}=await import('../src/react/agents/automation/pane.tsx');const {startLive}=await import('../src/react/chat/live.ts');
    await update(()=>{stopLive=startLive();});await render(AutomationPane,{agent});await click(button('View runs'));assert.match(document.body.textContent,/PRIVATE_DELIVERY_CHANNEL/);
    denied=true;await update(()=>stream.listeners.get('changes')({data:JSON.stringify({cursor:42,events:[{kind:'access'}]})}));
    assert.equal(document.body.textContent.includes('PRIVATE_DELIVERY_CHANNEL'),false);assert.equal(document.body.textContent.includes('PRIVATE_TASK_INSTRUCTIONS'),false);assert.equal(document.querySelector('dialog'),null);
    await update(()=>deferred.forEach(resolve=>resolve(response({error:'Access changed'},403))));assert.match(document.body.textContent,/Access changed/);
   }finally{stopLive?.();stopLive=undefined;}
  },{url,fetch:async path=>{if(path.endsWith('/message-capabilities'))return response({live:true});if(path.endsWith('/schedules')&&denied)return new Promise(resolve=>deferred.push(resolve));return base(path);}});
 }finally{globalThis.EventSource=original;}
});

test('routine review fetches full instructions only when opened and failed state writes retry the same versioned request',async()=>{
 const writes=[];let details=0;
 await reactFixture(async({render,button,click})=>{
  await setup();const {AutomationPane}=await import('../src/react/agents/automation/pane.tsx');
  await render(AutomationPane,{agent});assert.equal(details,0);assert.equal(document.body.textContent.includes('PRIVATE_FULL_INSTRUCTIONS'),false);
  await click(button('View instructions'));assert.ok(details>0);assert.match(document.body.textContent,/PRIVATE_FULL_INSTRUCTIONS/);assert.equal(writes.length,0);
  await click(button('Done'));await click(button('Pause'));assert.match(document.body.textContent,/PRIVATE_FULL_INSTRUCTIONS/);
  await click(button('Pause routine'));assert.match(document.body.textContent,/State result unavailable/);await click(button('Pause routine'));assert.deepEqual(writes[0],writes[1]);
 },{url,fetch:async(path,init={})=>{
  if(init.method==='POST'){writes.push(JSON.parse(init.body));return response({error:'State result unavailable'},503);}
  if(path.endsWith('/schedules/routine')){details++;return response({...routine,content:'PRIVATE_FULL_INSTRUCTIONS'});}
  return base(path);
 }});
});

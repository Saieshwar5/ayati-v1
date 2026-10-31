import { registerHooks } from 'node:module';
registerHooks({load(url,context,next){return url.endsWith('.css')?{format:'module',source:'export {};',shortCircuit:true}:next(url,context);}});
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { reactFixture } from './react-fixture.mjs';
const w='11111111-1111-4111-8111-111111111111',url=`http://localhost/w/${w}/?view=agents`;
const session={user:{id:'owner',email:'owner@example.test'},workspaces:[{id:w,name:'Store',role:'Admin'}]};
const agent={id:'agent',name:'Nova',role:'Help customers',model:'fixture',audience:'selected',revision:3,editable:true,settings:{provider:'openai',instructions:'Keep a clear tone',escalation:'When facts are missing',successful_outcome:'A clear sourced answer',routine:'Check supplied facts',memory:true,history:'all',avatar:'spark',browser:false,paused:false}};
const source={id:'source',revision:2,access_revision:3,title:'PRIVATE_DELIVERY_GUIDE',kind:'page',visibility:'private'};
const options={profile:agent,sources:[source],more:false,notes:[],execution_available:true,active_sample:null};
const sample={id:'sample',state:'running',detail:'Working on the draft-only sample',input:{id:'sample',agent_revision:3,task:'Draft a customer response',sources:[source]},profile:agent,sources:[source],note_refs:[],created_ms:1000,started_ms:2000,finished_ms:null};
const response=(data,status=200)=>({ok:status<400,status,json:async()=>data});
const history={samples:[{id:'old',state:'completed',task:'Old task',profile_revision:2,created_ms:1000},{id:'new',state:'completed',task:'New task',profile_revision:3,created_ms:2000}],next_before:null};
async function setup(){const context=await import('../src/workspace-context.ts');await context.enterWorkspace();return(await import('../src/react/agents/rehearsals/dialog.tsx')).RehearsalDialog;}
function baseline(path){if(path.endsWith('/session'))return response(session);if(path.includes('rehearsal-options'))return response(options);if(path.endsWith('/rehearsals'))return response({samples:[],next_before:null});return response(sample);}

test('sample context review is read-only and double Start saves one exact request with selected source versions',async()=>{
  const writes=[];let finish;
  await reactFixture(async({render,button,change,click,update})=>{
    const Dialog=await setup();await render(Dialog,{agent,close:()=>{}});assert.equal(writes.length,0);
    await change(document.querySelector('textarea'),'Draft a customer response');
    await update(()=>document.querySelector('input[type=checkbox]').click());
    await click(button('Review sample context'));assert.equal(writes.length,0);
    assert.match(document.body.textContent,/Keep a clear tone/);assert.match(document.body.textContent,/PRIVATE_DELIVERY_GUIDE · version 2/);
    await update(()=>{const node=button('Start draft-only sample');node.click();node.click();});
    assert.equal(writes.length,1);assert.equal(writes[0].agent_revision,3);assert.deepEqual(writes[0].sources,[{id:'source',revision:2,access_revision:3}]);
    await update(()=>finish(response({...sample,state:'queued'})));
    assert.match(document.body.textContent,/Working on sample/);assert.equal(writes.length,1);
  },{url,fetch:async(path,init={})=>{if(init.method==='POST'){writes.push(JSON.parse(init.body));return new Promise(resolve=>{finish=resolve;});}return baseline(path);}});
});

test('failed Start keeps the task and choices, retries only the exact payload, and changed tasks get a new identity',async()=>{
  const writes=[];
  await reactFixture(async({render,button,change,click,update})=>{
    const Dialog=await setup();await render(Dialog,{agent,close:()=>{}});
    await change(document.querySelector('textarea'),'KEEP_FAILED_SAMPLE_DRAFT');await update(()=>document.querySelector('input[type=checkbox]').click());
    await click(button('Review sample context'));await click(button('Start draft-only sample'));assert.match(document.body.textContent,/Sample temporarily unavailable/);
    await click(button('Start draft-only sample'));assert.equal(writes[0].id,writes[1].id);
    await click(button('Back to sample'));assert.equal(document.querySelector('textarea').value,'KEEP_FAILED_SAMPLE_DRAFT');assert.equal(document.querySelector('input[type=checkbox]').checked,true);
    await change(document.querySelector('textarea'),'Changed sample task');await click(button('Review sample context'));await click(button('Start draft-only sample'));
    assert.notEqual(writes[2].id,writes[0].id);assert.equal(writes.length,3);
  },{url,fetch:async(path,init={})=>{if(init.method==='POST'){writes.push(JSON.parse(init.body));return response({error:'Sample temporarily unavailable'},503);}return baseline(path);}});
});

test('saved history opens exact responses without inference and denied source refresh clears old private content',async()=>{
  let denied=false;const writes=[];
  const reference={schema:1,id:'old',version:'version',sequence:1,state:'complete',characters:2000,bytes:2000,long:true,available:true};
  await reactFixture(async({render,button,click})=>{
    const Dialog=await setup();await render(Dialog,{agent,close:()=>{}});await click(button('Saved samples'));
    await click(button('Open sample'));assert.match(document.body.textContent,/PRIVATE_RESULT_EXCERPT/);
    await click(button('Read response'));assert.match(document.querySelector('[aria-label="Saved response content"]').textContent,/PRIVATE_FULL_RESPONSE/);
    const reader=[...document.querySelectorAll('dialog')].find(node=>node.querySelector('h2').textContent==='Agent response');await click(reader.querySelector('[aria-label="Close"]'));
    denied=true;await click(button('Refresh sample status'));
    assert.equal(document.body.textContent.includes('PRIVATE_RESULT_EXCERPT'),false);assert.equal(document.body.textContent.includes('PRIVATE_DELIVERY_GUIDE'),false);
    assert.match(document.body.textContent,/Source changed/);assert.deepEqual(writes,[]);
  },{url,fetch:async(path,init={})=>{
    if(init.method==='POST')writes.push(path);
    if(path.endsWith('/rehearsals'))return response(history);
    if(path.includes('/rehearsals/old/response'))return response({...reference,body:'PRIVATE_FULL_RESPONSE',format:'markdown'});
    if(path.endsWith('/rehearsals/old'))return denied?response({error:'Source changed'},404):response({...sample,id:'old',state:'completed',preview:'PRIVATE_RESULT_EXCERPT',response:reference});
    return baseline(path);
  }});
});

test('a withdrawn-source active sample can be stopped through its personal minimal receipt',async()=>{
  const writes=[];let stopped=false;
  await reactFixture(async({render,button,click})=>{
    const Dialog=await setup();await render(Dialog,{agent,close:()=>{}});await click(button('Open active sample'));
    assert.match(document.body.textContent,/Sample unavailable/);assert.equal(document.body.textContent.includes('PRIVATE_DELIVERY_GUIDE'),false);
    await click(button('Stop sample'));assert.equal(writes.length,1);assert.match(writes[0],/\/rehearsals\/active\/stop$/);
    assert.equal(button('Stop sample'),undefined);
  },{url,fetch:async(path,init={})=>{
    if(path.includes('rehearsal-options'))return response({...options,sources:[],active_sample:stopped?null:{id:'active',state:'running'}});
    if(init.method==='POST'){writes.push(path);stopped=true;return response({id:'active',state:'cancelled'});}
    if(path.endsWith('/rehearsals/active'))return response({error:'Sample unavailable'},404);
    return baseline(path);
  }});
});

test('late reads cannot replace another selected sample or survive switching agents',async()=>{
  const pending=[];
  await reactFixture(async({render,button,click,update})=>{
    const Dialog=await setup();await render(Dialog,{agent,close:()=>{}});await click(button('Saved samples'));
    await click(button('Open sample'));await click(button('Saved samples'));
    await click([...document.querySelectorAll('button')].filter(node=>node.textContent==='Open sample')[1]);
    assert.match(document.body.textContent,/CURRENT_SAMPLE_RESULT/);
    await update(()=>pending.forEach(resolve=>resolve(response({...sample,id:'old',state:'completed',preview:'STALE_PRIVATE_RESULT'}))));
    assert.equal(document.body.textContent.includes('STALE_PRIVATE_RESULT'),false);
    await render(Dialog,{agent:{...agent,id:'different',name:'Other agent'},close:()=>{}});
    assert.equal(document.body.textContent.includes('CURRENT_SAMPLE_RESULT'),false);assert.equal(document.querySelector('textarea').value,'');
  },{url,fetch:async path=>{
    if(path.endsWith('/rehearsals'))return response(history);
    if(path.endsWith('/rehearsals/old'))return new Promise(resolve=>pending.push(resolve));
    if(path.endsWith('/rehearsals/new'))return response({...sample,id:'new',state:'completed',preview:'CURRENT_SAMPLE_RESULT'});
    return baseline(path);
  }});
});

test('late Start acknowledgments after closing cannot reopen a sample or issue follow-up reads',async()=>{
  let finish;const writes=[];
  await reactFixture(async({render,button,change,click,update})=>{
    const Dialog=await setup();await render(Dialog,{agent,close:()=>{}});await change(document.querySelector('textarea'),'A sample task');
    await click(button('Review sample context'));await click(button('Start draft-only sample'));
    await render(()=>createElement('p',null,'Closed setup'));
    await update(()=>finish(response(sample)));assert.equal(document.querySelector('dialog'),null);assert.equal(writes.length,1);
  },{url,fetch:async(path,init={})=>{if(init.method==='POST'){writes.push(path);return new Promise(resolve=>{finish=resolve;});}return baseline(path);}});
});

test('editing instructions stays an explicit profile save with existing authority fields preserved',async()=>{
  const writes=[];
  await reactFixture(async({render,button,change,click})=>{
    await setup();const {AgentEditor}=await import('../src/react/agents/editor.tsx');
    await render(AgentEditor,{agent,directory:{agents:[agent],role:'Admin',providers:[],default_model:'fixture',system_prompt:'System guide'},creating:false,close:()=>{},saved:async()=>{}});
    await click(button('Instructions'));
    const text=[...document.querySelectorAll('label')].find(node=>node.textContent.startsWith('Instructions')).querySelector('textarea');
    await change(text,'Verify the supplied delivery facts');assert.equal(writes.length,0);
    await click(button('Save changes'));assert.equal(writes.length,1);
    assert.equal(writes[0].settings.instructions,'Verify the supplied delivery facts');
    assert.equal(writes[0].settings.provider,'openai');assert.equal(writes[0].settings.memory,true);assert.equal(writes[0].settings.browser,false);
  },{url,fetch:async(path,init={})=>{
    if(init.method==='POST'){writes.push(JSON.parse(init.body));return response({...agent,revision:4});}
    if(path.endsWith('/knowledge'))return response({notes:[]});if(path.endsWith('/chats'))return response({chats:[]});return baseline(path);
  }});
});

test('personal source invalidation clears an open reader and sample before a delayed revalidation can return',async()=>{
  let stream,denied=false,stopLive;const deferred=[];
  const original=globalThis.EventSource;
  const reference={schema:1,id:'old',version:'version',sequence:1,state:'complete',characters:2000,bytes:2000,long:true,available:true};
  try {
    globalThis.EventSource=class {constructor(){this.listeners=new Map();stream=this;}addEventListener(name,fn){this.listeners.set(name,fn);}close(){}};
    await reactFixture(async({render,button,click,update})=>{
      try {
      const Dialog=await setup();const {startLive}=await import('../src/react/chat/live.ts');
      await update(()=>{stopLive=startLive();});
      await render(Dialog,{agent,close:()=>{}});await click(button('Saved samples'));await click(button('Open sample'));
      await click(button('Read response'));assert.match(document.body.textContent,/PRIVATE_FULL_SOURCE_RESPONSE/);
      denied=true;
      await update(()=>stream.listeners.get('changes')({data:JSON.stringify({cursor:12,events:[{kind:'sample_access'}]})}));
      assert.equal(document.querySelector('[aria-label="Saved response content"]'),null);
      assert.equal(document.body.textContent.includes('PRIVATE_SAMPLE_EXCERPT'),false);
      assert.equal(document.body.textContent.includes('PRIVATE_DELIVERY_GUIDE'),false);
      await update(()=>deferred.forEach(resolve=>resolve(response({error:'Selected knowledge changed'},404))));
      assert.match(document.body.textContent,/Selected knowledge changed/);
      } finally {stopLive?.();stopLive=undefined;}
    },{url,fetch:async path=>{
      if(path.endsWith('/message-capabilities'))return response({live:true});
      if(path.endsWith('/rehearsals'))return response(denied?{samples:[],next_before:null}:history);
      if(path.includes('rehearsal-options'))return response({...options,sources:denied?[]:[source]});
      if(path.endsWith('/rehearsals/old'))return denied?new Promise(resolve=>deferred.push(resolve)):response({...sample,id:'old',state:'completed',preview:'PRIVATE_SAMPLE_EXCERPT',response:reference});
      if(path.includes('/rehearsals/old/response'))return response({...reference,body:'PRIVATE_FULL_SOURCE_RESPONSE',format:'markdown'});
      return baseline(path);
    }});
  } finally {globalThis.EventSource=original;}
});

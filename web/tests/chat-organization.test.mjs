import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement, useRef } from 'react';
import { reactFixture } from './react-fixture.mjs';
const w='11111111-1111-4111-8111-111111111111',url=`http://localhost/w/${w}/?view=chat`;
const session={user:{id:'owner',email:'owner@example.test'},workspaces:[{id:w,name:'Store',role:'Owner'}]};
const directory={users:[{id:'owner',name:'Ananya'}],agents:[],role:'Owner'};
const chat={id:'operations',name:'Operations',kind:'channel',users:['owner'],agents:[],description:'Work together',can_archive:true};
const initial={available:true,revision:0,pins:[]};
const response=(data,status=200)=>({ok:status<400,status,json:async()=>data});
async function start(){await(await import('../src/workspace-context.ts')).enterWorkspace();}
test('personal pin mutations are explicit, guard double sends, retain exact retry identity and move one row without starting a conversation',async()=>{
  const writes=[];let finish;
  await reactFixture(async({render,button,update,click})=>{
    await start();const {WorkspaceChat}=await import('../src/react/chat/workspace-chat.tsx');
    await render(WorkspaceChat,{href:url});assert.equal(writes.length,0);
    await update(()=>{button('Pin Operations').click();button('Pin Operations').click();});assert.equal(writes.length,1);
    await update(()=>finish(response({error:'Temporary save failure'},503)));assert.match(document.body.textContent,/Temporary save failure/);
    await click(button('Pin Operations'));assert.equal(writes[0].request,writes[1].request);
    await update(()=>finish(response({...initial,revision:1,pins:writes[1].pins})));
    assert.equal(document.querySelector('[aria-label="Pinned conversations"] .chat-contact').textContent,'#Operations');
    assert.equal([...document.querySelectorAll('a.chat-contact')].filter(a=>a.textContent.includes('Operations')).length,1);
    assert.equal(writes.every(write=>write.pins[0].kind==='chat'),true);
    await click(button('Unpin Operations'));assert.notEqual(writes[2].request,writes[1].request);
    await update(()=>finish(response({...initial,revision:2})));
    assert.equal(document.querySelector('[aria-label="Pinned conversations"]'),null);
  },{url,fetch:async(path,options={})=>{
    if(options.method==='POST'){assert.match(path,/\/chat-pins$/);writes.push(JSON.parse(options.body));return new Promise(resolve=>{finish=resolve;});}
    return response(path.endsWith('/session')?session:path.endsWith('/chat-directory')?directory:path.endsWith('/chats')?{chats:[chat],preferences:initial}:path.endsWith('/unread')?{unread:[]}:{agents:[]});
  }});
});
test('archived pins leave active Pinned, remain in Archived and preserve their personal pin preference',async()=>{
  await reactFixture(async({render,button,click})=>{
    await start();const {WorkspaceChat}=await import('../src/react/chat/workspace-chat.tsx');
    await render(WorkspaceChat,{href:url});assert.equal(document.querySelector('[aria-label="Pinned conversations"]'),null);
    assert.equal(document.querySelector('a.chat-contact'),null);
    await click(button('Archived channels (1)'));
    assert.match(document.querySelector('a.chat-contact').textContent,/OperationsArchived/);
    assert.ok(button('Unpin Operations'));
  },{url,fetch:async path=>response(path.endsWith('/session')?session:path.endsWith('/chat-directory')?directory:path.endsWith('/chats')?{chats:[{...chat,archived:true}],preferences:{...initial,revision:1,pins:[{kind:'chat',id:chat.id}]}}:path.endsWith('/unread')?{unread:[]}:{agents:[]})});
});
test('archive review reads current unfinished work, submits explicitly once and preserves a failed exact request for retry',async()=>{
  let finish,changed=0;const writes=[];
  await reactFixture(async({render,button,click,update})=>{
    await start();const {ChannelLifecycle}=await import('../src/react/chat/channel-lifecycle.tsx');
    await render(ChannelLifecycle,{chat,close:()=>{},changed:async()=>{changed++;}});
    assert.equal(writes.length,0);assert.match(document.body.textContent,/2 visible unfinished requests/);
    await update(()=>{button('Archive channel').click();button('Archive channel').click();});assert.equal(writes.length,1);
    await update(()=>finish(response({error:'Save unconfirmed'},503)));
    await click(button('Archive channel'));assert.equal(writes[0].request,writes[1].request);assert.equal(writes[1].archived,true);assert.equal(writes[1].revision,4);
    await update(()=>finish(response({archived:true,revision:5})));assert.equal(changed,1);
  },{url,fetch:async(path,options={})=>{if(options.method==='POST'){writes.push(JSON.parse(options.body));return new Promise(resolve=>{finish=resolve;});}return response(path.endsWith('/session')?session:{archived:false,revision:4,active_requests:2,manageable:true});}});
});
test('denied lifecycle refresh clears management controls and cannot authorize an archive',async()=>{
  let denied=false,writes=0;
  await reactFixture(async({render,button,click})=>{
    await start();const {ChannelLifecycle}=await import('../src/react/chat/channel-lifecycle.tsx');await render(ChannelLifecycle,{chat,close:()=>{},changed:async()=>{}});
    denied=true;await click(button('Refresh channel state'));assert.equal(button('Archive channel').disabled,true);assert.match(document.body.textContent,/Source revoked/);assert.doesNotMatch(document.body.textContent,/Operations/);assert.equal(writes,0);
  },{url,fetch:async(path,options={})=>{if(options.method==='POST')writes++;return path.endsWith('/session')?response(session):denied?response({error:'Source revoked'},403):response({archived:false,revision:0,active_requests:1,manageable:true});}});
});
test('a late pin acknowledgment after a source change triggers a fresh read instead of restoring stale pins',async()=>{
  let finish,epoch,saved=0,refreshed=0;
  await reactFixture(async({render,button,click,update})=>{
    await start();const {usePinActions}=await import('../src/react/chat/pin-control.tsx');
    function Harness(){epoch=useRef(0);const pins=usePinActions(initial,()=>{saved++;},epoch,async()=>{refreshed++;});return createElement('button',{onClick:()=>pins.toggle({kind:'chat',id:chat.id})},'Pin');}
    await render(Harness);await click(button('Pin'));epoch.current++;
    await update(()=>finish(response({...initial,revision:1,pins:[{kind:'chat',id:chat.id}]})));
    assert.equal(saved,0);assert.equal(refreshed,1);
  },{url,fetch:async(path,options={})=>options.method==='POST'?new Promise(resolve=>{finish=resolve;}):response(session)});
});

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';
import { outboxStorage } from './outbox-storage-fixture.mjs';
const w='11111111-1111-4111-8111-111111111111';
const until=async(fn)=>{for(let i=0;i<50;i++){if(fn())return;await new Promise(r=>setImmediate(r));}assert.ok(fn());};
test('outbox retains uncertain delivery across reload and retries exact IDs; definite rejection stays actionable',async()=>{
 const previous=globalThis.indexedDB;globalThis.indexedDB=outboxStorage();
 let mode='lost',actor='owner';const calls=[];
 try{await reactFixture(async()=>{
  const context=await import('../src/workspace-context.ts');await context.enterWorkspace();
  const box=await import('../src/react/chat/outbox.ts');
  try{
   const payload={id:crypto.randomUUID(),content:'Keep this once',attachments:[]};
   await box.enqueue('chat',payload);
   await until(()=>box.outboxSnapshot()[0]?.state==='checking');
   assert.equal(box.outboxSnapshot()[0].payload.id,payload.id);
   box.stopOutbox();await box.loadOutbox();assert.equal(box.outboxSnapshot()[0].state,'checking');
   mode='ok';await box.retryMessage(box.outboxSnapshot()[0]);await until(()=>box.outboxSnapshot().length===0);
   assert.equal(calls.length,2);assert.deepEqual(calls[0],calls[1]);
   mode='ok-only';const confirmed={...payload,id:crypto.randomUUID()};await box.enqueue('chat',confirmed);
   await until(()=>box.outboxSnapshot()[0]?.state==='sent');
   await box.confirmSent([{...confirmed,author:'owner'}]);assert.equal(box.outboxSnapshot().length,0);
   mode='denied';await box.enqueue('chat',{...payload,id:crypto.randomUUID(),content:'Access removed'});
   await until(()=>box.outboxSnapshot()[0]?.state==='failed');
   assert.match(box.outboxSnapshot()[0].error,/Access removed/);
   const before=calls.length;await box.flushOutbox();assert.equal(calls.length,before,'Definite rejection must not loop automatically');
   box.stopOutbox();actor='another-user';await context.enterWorkspace();await box.loadOutbox();
   assert.equal(box.outboxSnapshot().length,0,'Another account must not see or submit the original outbox');
   await box.flushOutbox();assert.equal(calls.length,before);
  }finally{box.stopOutbox();}
 },{url:`http://localhost/w/${w}/`,fetch:async(path,options={})=>{
  if(path.endsWith('/session'))return {ok:true,status:200,json:async()=>({user:{id:actor,email:'owner@example.test'},workspaces:[{id:w,name:'Test',role:'Owner'}]})};
  calls.push(JSON.parse(options.body));
  if(mode==='lost')throw new TypeError('Connection dropped after commit');
  return {ok:mode.startsWith('ok'),status:mode.startsWith('ok')?200:403,json:async()=>mode==='ok'?{ok:true,message:{...calls.at(-1),author:'owner',kind:'user',state:'sent',at:1,chat:'chat'}}:mode==='ok-only'?{ok:true}:{error:'Access removed'}};
 }});}finally{globalThis.indexedDB=previous;}
});

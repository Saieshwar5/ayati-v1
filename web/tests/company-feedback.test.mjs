import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';
const w='11111111-1111-4111-8111-111111111111';
const response=(data,ok=true)=>({ok,status:ok?200:400,json:async()=>data});
test('feedback retains failed drafts, gates duplicate saves, and reuses only the exact payload id',async()=>{
 let release,calls=[],fail=true;
 await reactFixture(async({render,button,click,change,submit,update})=>{
  const context=await import('../src/workspace-context.ts');await context.enterWorkspace();
  const {FeedbackPage}=await import('../src/react/feedback.tsx');await render(FeedbackPage);
  await click(button('Report an issue'));
  const inputs=[...document.querySelectorAll('textarea')];await change(inputs[0],'Upload failed');await change(inputs[1],'File is saved');await change(inputs[2],'Upload returns an error');
  await submit(document.querySelector('form'));await submit(document.querySelector('form'));
  assert.equal(calls.length,1);await update(()=>release(response({error:'Disk unavailable'},false)));
  assert.equal(inputs[0].value,'Upload failed');assert.match(document.body.textContent,/Disk unavailable/);
  await submit(document.querySelector('form'));assert.equal(calls.length,2);assert.equal(calls[1].id,calls[0].id);
  await update(()=>release(response({error:'Still unavailable'},false)));await change(inputs[2],'Upload times out');await submit(document.querySelector('form'));
  assert.notEqual(calls[2].id,calls[1].id);fail=false;await update(()=>release(response({ok:true})));
  assert.equal(document.querySelector('dialog'),null);
 },{url:`http://localhost/w/${w}/?view=feedback`,fetch:async(path,options={})=>{
  if(path.endsWith('/session'))return response({user:{id:'owner',email:'owner@example.test'},workspaces:[{id:w,name:'Test',role:'Owner'}]});
  if(path.endsWith('/chats'))return response({chats:[{id:'chat',name:'Operations'}]});
  if(path.endsWith('/feedback')&&options.method==='POST'){calls.push(JSON.parse(options.body));return new Promise(r=>{release=r;});}
  if(path.endsWith('/feedback'))return response({feedback:fail?[]:[{...calls.at(-1),status:'new',revision:0}]});
  throw new Error(path);
 }});
});

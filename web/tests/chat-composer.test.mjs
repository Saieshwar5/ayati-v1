import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format:'module',source:'export {};',shortCircuit:true } : next(url,context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { useState, createElement } from 'react';
import { reactFixture } from './react-fixture.mjs';
import { decodeMentions, editMentions, requestAgent } from '../src/react/chat/mentions.ts';

test('mention edits preserve stable identities only while their visible names remain intact', () => {
 const source='Hello [@Sam](mention:user:u1), ask [@Sam](mention:agent:a1)';
 assert.equal(decodeMentions(source).text,'Hello @Sam, ask @Sam');
 assert.equal(requestAgent(source),'a1');
 assert.equal(requestAgent(editMentions(source,'Hello @Sam, ask @Someone')),undefined);
 const moved=editMentions(source,'Well, Hello @Sam, ask @Sam');
 assert.deepEqual(decodeMentions(moved).mentions.map(m=>m.id),['u1','a1']);
 assert.throws(()=>requestAgent(source+' [@Other](mention:agent:a2)'),/one agent/);
 assert.equal(requestAgent('email@example.com and @Sam'),undefined);
});

test('inline suggestions select identities with keyboard, preserve newlines, and never submit during composition', async()=>{
 let sent=[];
 await reactFixture(async({render,change,update,submit,dom})=>{
  const { Composer }=await import('../src/react/chat/composer.tsx');
  function Harness(){const [value,set]=useState('');return createElement(Composer,{value,change:set,send:()=>sent.push(value),candidates:[{id:'u1',name:'Sam',kind:'user'},{id:'a1',name:'Sam',kind:'agent'}]});}
  await render(Harness);const input=document.querySelector('textarea');
  await change(input,'@Sa');assert.equal(document.querySelectorAll('[role=option]').length,2);
  await update(()=>input.dispatchEvent(new dom.window.KeyboardEvent('keydown',{key:'ArrowDown',bubbles:true})));
  await update(()=>input.dispatchEvent(new dom.window.KeyboardEvent('keydown',{key:'Enter',bubbles:true})));
  assert.equal(input.value,'@Sam ');assert.equal(sent.length,0);
  await update(()=>input.dispatchEvent(new dom.window.KeyboardEvent('keydown',{key:'Enter',isComposing:true,bubbles:true})));assert.equal(sent.length,0);
  await update(()=>input.dispatchEvent(new dom.window.KeyboardEvent('keydown',{key:'Enter',shiftKey:true,bubbles:true})));assert.equal(sent.length,0);
  await submit(document.querySelector('form'));assert.equal(sent[0],'[@Sam](mention:agent:a1) ');
  await change(input,'@Sam edited');await submit(document.querySelector('form'));assert.equal(requestAgent(sent[1]),'a1');
  await change(input,'@Someone edited');await submit(document.querySelector('form'));assert.equal(requestAgent(sent[2]),undefined);
 });
});

test('cancelled microphone requests release late streams without entering recording',async()=>{
 const navigatorDescriptor=Object.getOwnPropertyDescriptor(globalThis,'navigator');
 const oldRecorder=globalThis.MediaRecorder;
 let resolveMedia,stopped=0,started=0;
 Object.defineProperty(globalThis,'navigator',{configurable:true,value:{mediaDevices:{getUserMedia:()=>new Promise(resolve=>{resolveMedia=resolve;})}}});
 globalThis.MediaRecorder=class {constructor(){started++;}};
 try {
  await reactFixture(async({render,button,click,update})=>{
   const { VoiceRecorder }=await import('../src/react/chat/voice-recorder.tsx');let pending=false;
   await render(VoiceRecorder,{disabled:false,attach:async()=>{},pending:value=>{pending=value;}});
   await click(button('Record voice'));assert.equal(pending,true);
   await click(button('Cancel recording'));assert.equal(pending,false);
   await update(()=>resolveMedia({getTracks:()=>[{stop:()=>{stopped++;}}]}));
   assert.equal(stopped,1);assert.equal(started,0);assert.ok(button('Record voice'));
  });
 }finally{if(navigatorDescriptor)Object.defineProperty(globalThis,'navigator',navigatorDescriptor);else delete globalThis.navigator;globalThis.MediaRecorder=oldRecorder;}
});

test('Stop attaches once, failed upload retains audio for retry, and Cancel never attaches', async()=>{
 const descriptor=Object.getOwnPropertyDescriptor(globalThis,'navigator'), old=globalThis.MediaRecorder;
 let attempts=0, fail=true, stopped=0;
 Object.defineProperty(globalThis,'navigator',{configurable:true,value:{mediaDevices:{getUserMedia:async()=>({getTracks:()=>[{stop:()=>stopped++}]})}}});
 globalThis.MediaRecorder=class {
  state='inactive';mimeType='audio/webm';
  start(){this.state='recording';}
  stop(){this.state='inactive';this.ondataavailable({data:new Blob(['audio'],{type:this.mimeType})});this.onstop();}
 };
 try {await reactFixture(async({render,button,click})=>{
  const {VoiceRecorder}=await import('../src/react/chat/voice-recorder.tsx');let pending=false;
  await render(VoiceRecorder,{disabled:false,attach:async()=>{attempts++;if(fail)throw Error('offline');},pending:v=>pending=v});
  await click(button('Record voice'));await click(button('Stop recording'));
  assert.equal(attempts,1);assert.equal(pending,true);assert.ok(document.querySelector('audio'));
  fail=false;await click(button('Retry voice upload'));assert.equal(attempts,2);assert.equal(pending,false);assert.equal(document.querySelector('audio'),null);
  await click(button('Record voice'));await click(button('Cancel recording'));assert.equal(attempts,2);assert.equal(pending,false);assert.ok(stopped>=2);
 });}finally{if(descriptor)Object.defineProperty(globalThis,'navigator',descriptor);else delete globalThis.navigator;globalThis.MediaRecorder=old;}
});

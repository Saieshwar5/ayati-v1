import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format:'module',source:'export {};',shortCircuit:true } : next(url,context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { useState, createElement } from 'react';
import { reactFixture } from './react-fixture.mjs';
import { decodeMentions, editMentions, requestAgents } from '../src/react/chat/mentions.ts';
const requestAgent=source=>requestAgents(source)[0];

test('mention edits preserve stable identities only while their visible names remain intact', () => {
 const source='Hello [@Sam](mention:user:u1), ask [@Sam](mention:agent:a1)';
 assert.equal(decodeMentions(source).text,'Hello @Sam, ask @Sam');
 assert.equal(requestAgent(source),'a1');
 assert.equal(requestAgent(editMentions(source,'Hello @Sam, ask @Someone')),undefined);
 const moved=editMentions(source,'Well, Hello @Sam, ask @Sam');
 assert.deepEqual(decodeMentions(moved).mentions.map(m=>m.id),['u1','a1']);
 assert.equal(requestAgent('email@example.com and @Sam'),undefined);
});

test('a message asks every agent it mentions, up to three, and a direct message only its own agent', () => {
 const two='[@Atlas](mention:agent:a1) and [@Nova](mention:agent:n1), stock? [@Atlas](mention:agent:a1)';
 assert.deepEqual(requestAgents(two),['a1','n1'],'once each, in the order mentioned');
 const three=two+' [@Orion](mention:agent:o1)';
 assert.deepEqual(requestAgents(three),['a1','n1','o1']);
 assert.throws(()=>requestAgents(three+' [@Vega](mention:agent:v1)'),/Ask at most 3 agents in one message/);
 assert.deepEqual(requestAgents('No agent here [@Sam](mention:user:u1)'),[]);
 assert.deepEqual(requestAgents('Hello','d1'),['d1']);
 assert.deepEqual(requestAgents('Hello [@Dee](mention:agent:d1)','d1'),['d1']);
 assert.throws(()=>requestAgents(two,'d1'),/A direct message asks only its own agent/);
});

test('the picker offers more agents until three are asked, then says why it offers no more', async()=>{
 await reactFixture(async({render,change,click})=>{
  const { Composer }=await import('../src/react/chat/composer.tsx');
  const agents=['Atlas','Nova','Orion','Vega'].map(name=>({id:name.toLowerCase(),name,kind:'agent'}));
  function Harness(){const [value,set]=useState('');return createElement(Composer,{value,change:set,send:()=>{},candidates:agents});}
  await render(Harness);const input=document.querySelector('textarea');
  const options=()=>[...document.querySelectorAll('[role=option]')].map(o=>[o.querySelector('.chat-name-text').textContent,o.querySelector('small').textContent,o.getAttribute('aria-disabled')]);
  await change(input,'@At');assert.deepEqual(options(),[['Atlas','Asks this agent to work',null]]);
  await click(document.querySelector('[role=option]'));
  for(const name of ['Nova','Orion']){
   await change(input,`${input.value}@${name.slice(0,2)}`);
   assert.deepEqual(options(),[[name,'Also asks this agent',null]]);
   await click(document.querySelector('[role=option]'));
  }
  assert.equal(input.value,'@Atlas @Nova @Orion ');
  await change(input,`${input.value}@Ve`);
  assert.deepEqual(options(),[['Vega','Up to 3 agents per message','true']]);
  await click(document.querySelector('[role=option]'));
  assert.equal(input.value,'@Atlas @Nova @Orion @Ve','a fourth agent is not added');
  await change(input,'@Atlas @Nova @Orion @At');
  assert.deepEqual(options(),[['Atlas','Already asked',null]]);
 });
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

// Recording itself is covered in tests/voice-recorder.test.mjs; this is the composer's side.
test('a voice row replaces the text field: Enter sends, Escape cancels, buttons keep their own keys, and typing resumes after', async()=>{
 const calls=[];
 await reactFixture(async({render,update,change,dom})=>{
  const { Composer }=await import('../src/react/chat/composer.tsx');
  let setVoice;
  function Harness(){
   const [value,set]=useState('draft kept');const [voice,choose]=useState();setVoice=choose;
   return createElement(Composer,{value,change:set,send:()=>calls.push(`send:${value}`),voice,candidates:[{id:'a1',name:'Atlas',kind:'agent'}],
    media:createElement('button',{className:'ui-button',type:'button'},'Attach')});
  }
  await render(Harness);
  const input=document.querySelector('textarea');
  await change(input,'@At');assert.equal(document.querySelectorAll('[role=option]').length,1);
  const row=createElement('div',{className:'voice-row',tabIndex:-1},createElement('button',{type:'button',onClick:()=>calls.push('row button')},'Row action'));
  await update(()=>setVoice({row,cancel:()=>calls.push('cancel')}));
  assert.equal(input.hidden,true);assert.equal(input.value,'@At','the draft stays while recording');
  assert.equal(document.querySelectorAll('[role=option]').length,0,'no mention menu over the recording');
  assert.ok(document.querySelector('form').classList.contains('is-recording'));
  const press=(target,key)=>{const event=new dom.window.KeyboardEvent('keydown',{key,bubbles:true,cancelable:true});target.dispatchEvent(event);return event.defaultPrevented;};
  document.querySelector('.voice-row').focus();
  let handled;
  await update(()=>{handled=press(document.querySelector('.voice-row'),'Enter');});
  assert.equal(handled,true);assert.deepEqual(calls,['send:@At']);
  await update(()=>{handled=press(document.querySelector('.voice-row button'),'Enter');});
  assert.equal(handled,false,'Enter on a button activates that button');assert.equal(calls.length,1);
  await update(()=>{handled=press(document.querySelector('.voice-row'),'Escape');});
  assert.equal(handled,true);assert.deepEqual(calls.slice(1),['cancel']);
  // While sending there is nothing to cancel.
  await update(()=>setVoice({row}));
  await update(()=>{handled=press(document.querySelector('.voice-row'),'Escape');});
  assert.equal(handled,false);assert.equal(calls.length,2);
  document.querySelector('.voice-row').focus();
  await update(()=>setVoice(undefined));
  assert.equal(input.hidden,false);
  assert.ok(document.activeElement===input,'focus returns to the text field');
 });
});

test('the emoji picker inserts at the cursor, keeps mentions, searches, and closes with Escape', async()=>{
 let sent=[];
 await reactFixture(async({render,change,update,click,submit,button,dom})=>{
  const { Composer }=await import('../src/react/chat/composer.tsx');
  function Harness(){const [value,set]=useState('');return createElement(Composer,{value,change:set,send:()=>sent.push(value),candidates:[{id:'a1',name:'Atlas',kind:'agent'}]});}
  await render(Harness);const input=document.querySelector('textarea');
  await change(input,'@Atl');
  await update(()=>input.dispatchEvent(new dom.window.KeyboardEvent('keydown',{key:'Enter',bubbles:true})));
  await change(input,'@Atlas on my way');
  await update(()=>input.setSelectionRange(7,7));
  await click(button('Add emoji'));assert.ok(document.querySelector('[role=dialog][aria-label=Emoji]'));
  await change(document.querySelector('input[type=search]'),'truck');
  assert.deepEqual([...document.querySelectorAll('.emoji-grid button')].map(b=>b.textContent),['🚚']);
  await click(button('truck'));
  assert.equal(input.value,'@Atlas 🚚on my way');assert.equal(document.querySelector('[aria-label=Emoji]'),null);
  await submit(document.querySelector('form'));assert.equal(requestAgent(sent[0]),'a1','the mention survives the insertion');
  await click(button('Add emoji'));
  await update(()=>document.querySelector('input[type=search]').dispatchEvent(new dom.window.KeyboardEvent('keydown',{key:'Escape',bubbles:true})));
  assert.equal(document.querySelector('[aria-label=Emoji]'),null);assert.equal(document.activeElement,button('Add emoji'));
 });
});

test('quote previews keep real text, mark deleted and unloaded messages, and shorten long ones', async()=>{
 const { quotePreview }=await import('../src/react/chat/quoted-message.tsx');
 const agents=id=>id==='a1'?'Nova':undefined;
 assert.deepEqual(quotePreview({id:'m',kind:'agent',author:'a1',content:'**GOOSE_FIXTURE_OK** [@Sam](mention:user:u1)',state:'complete'},agents),{name:'Nova',text:'GOOSE_FIXTURE_OK @Sam'});
 assert.equal(quotePreview({id:'m',kind:'user',author:'u1',name:'Sam',content:'x',state:'deleted'},agents).text,'Message deleted');
 assert.deepEqual(quotePreview(undefined,agents),{name:'Earlier message',text:'Not loaded in this view'});
 assert.equal(quotePreview({id:'m',kind:'user',author:'u1',name:'Sam',content:'y'.repeat(300),state:'sent'},agents).text.length,140);
});

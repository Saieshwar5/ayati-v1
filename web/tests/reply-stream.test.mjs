import { registerHooks } from 'node:module';
registerHooks({load(url,context,next){return url.endsWith('.css')?{format:'module',source:'export {};',shortCircuit:true}:next(url,context);}});
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';
import { mergeMessages } from '../src/react/chat/message-state.ts';

const workspace='11111111-1111-4111-8111-111111111111';
const session={user:{id:'owner',email:'owner@example.test'},workspaces:[{id:workspace,name:'Store',role:'Admin'}]};
const message={id:'reply',chat:'chat',author:'agent',kind:'agent',name:'Nova',content:'',state:'queued',at:1,sequence:2,revision:4,can_cancel:true,progress:{state:'responding',text:'Partial 🦀',generation:1}};
const response=(data,status=200)=>({ok:status<400,status,json:async()=>data});
// Re-rendering keeps an open menu open; only open it when it is closed.
const openMenu=async(button,click)=>{if(!button('Copy link'))await click(button('More actions'));};

test('replayed drafts and stale snapshots cannot overwrite newer output or a final reply',()=>{
 const final={...message,revision:9,state:'complete',progress:null,content:'Final answer'};
 assert.deepEqual(mergeMessages([final],[message]),[final]);
 const newer={...message,revision:5,progress:{...message.progress,text:'Partial 🦀 extended'}};
 assert.deepEqual(mergeMessages([newer],[message]),[newer]);
 const retry={...message,revision:7,progress:{state:'starting',text:'',generation:2}};
 assert.deepEqual(mergeMessages([message],[retry],[newer]),[retry]);
 assert.equal(mergeMessages([message],[{...message,delivered:3}],[{...message,seen:1}])[0].delivered,3);
});

test('draft text is provisional, retry reads as typing, and Cancel request guards duplicate submissions while retaining errors',async()=>{
 let finish;const writes=[];
 await reactFixture(async({render,button,update,click})=>{
  const context=await import('../src/workspace-context.ts');await context.enterWorkspace();
  const {ChatMessage}=await import('../src/react/chat/message.tsx');
  const props={message,directory:{users:[],agents:[{id:'agent',name:'Nova'}],role:'Admin'}};
  await render(ChatMessage,props);assert.match(document.body.textContent,/Partial 🦀/);assert.match(document.body.textContent,/Nova is typing/);
  assert.equal(button('Stop reply'),undefined,'no Stop button in the conversation');
  await click(button('More actions'));
  await update(()=>{const cancel=button('Cancel request');cancel.click();cancel.click();});assert.equal(writes.length,1);
  await update(()=>finish(response({error:'Temporarily unavailable'},503)));
  assert.match(document.querySelector('[role=alert]').textContent,/Temporarily unavailable/);assert.match(document.body.textContent,/Partial 🦀/);
  await click(button('More actions'));await click(button('Cancel request'));assert.equal(writes.length,2);
  await update(()=>finish(response({ok:true})));
  await render(ChatMessage,{...props,message:{...message,revision:7,progress:{state:'starting',generation:2,text:''}}});
  assert.equal(document.body.textContent.includes('Partial 🦀'),false);assert.match(document.body.textContent,/Nova is typing/);
  assert.equal(document.body.textContent.includes('Retrying'),false,'retries stay invisible');
  await render(ChatMessage,{...props,message:{...message,state:'cancelled',progress:null,content:'Request cancelled.'}});
  await click(button('More actions'));assert.equal(button('Cancel request'),undefined);assert.match(document.body.textContent,/Request cancelled/);
 },{url:`http://localhost/w/${workspace}/?view=chat`,fetch:async(path,options={})=>{
  if(options.method==='POST'){writes.push({path,body:JSON.parse(options.body)});return new Promise(resolve=>{finish=resolve;});}
  return response(session);
 }});
 assert.ok(writes.every(w=>w.path.endsWith('/chats/chat/messages/reply/cancel')));
});

test('people who did not ask see typing without Cancel request, and late cancel responses do not resurrect another view',async()=>{
 let finish;
 await reactFixture(async({render,button,click,update})=>{
  const context=await import('../src/workspace-context.ts');await context.enterWorkspace();
  const {ChatMessage}=await import('../src/react/chat/message.tsx');
  const directory={users:[],agents:[{id:'agent',name:'Nova'}],role:'Member'};
  await render(ChatMessage,{message:{...message,can_cancel:false},directory});
  assert.match(document.body.textContent,/Nova is typing/);await openMenu(button,click);assert.equal(button('Cancel request'),undefined);
  await render(ChatMessage,{message,directory});await openMenu(button,click);await click(button('Cancel request'));
  await render(()=>null);await update(()=>finish(response({error:'Denied'},403)));
  assert.equal(document.body.textContent.includes('Denied'),false);
 },{url:`http://localhost/w/${workspace}/?view=chat`,fetch:async(path,options={})=>options.method==='POST'?new Promise(resolve=>{finish=resolve;}):response(session)});
});

test('waits that need a person keep an honest status, other work reads as typing, and Cancel stays available without creating work',async()=>{
 const writes=[];
 await reactFixture(async({render,button,click})=>{
  const context=await import('../src/workspace-context.ts');await context.enterWorkspace();
  const {ChatMessage}=await import('../src/react/chat/message.tsx');
  const directory={users:[],agents:[{id:'agent',name:'Nova'}],role:'Admin'};
  for(const [state,label] of [['approval','Waiting for approval'],['resuming','Nova is typing'],['attention','Needs attention · External outcome not yet confirmed']]) {
   await render(ChatMessage,{message:{...message,progress:{state,text:''}},directory});
   assert.ok(document.querySelector('[role=status]').textContent.includes(label),state);
   await openMenu(button,click);assert.ok(button('Cancel request'),state);
  }
  assert.equal(writes.length,0);
 },{url:`http://localhost/w/${workspace}/?view=chat`,fetch:async(path,options={})=>{if(options.method==='POST')writes.push(path);return response(session);}});
});

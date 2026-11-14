import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement, Fragment } from 'react';
import { reactFixture } from './react-fixture.mjs';
const w='11111111-1111-4111-8111-111111111111',url=`http://localhost/w/${w}/?view=chat&conversation=orders`;
const session={user:{id:'meera',email:'meera@example.test'},workspaces:[{id:w,name:'Studio',role:'Member'}]};
const directory={role:'Member',users:[{id:'meera',name:'Meera Iyer',title:'',photo:null},{id:'ravi',name:'Ravi Kumar',title:'',photo:null}],agents:[]};
const orders={id:'orders',name:'orders',kind:'channel',visibility:'public',users:['meera','ravi'],agents:[],description:'',can_archive:true};
const messages=[
  {id:'mine',chat:'orders',author:'meera',name:'Meera Iyer',kind:'user',content:'Dispatch at noon',state:'sent',at:1,reactions:[{emoji:'👍',count:2,mine:true}]},
  {id:'theirs',chat:'orders',author:'ravi',name:'Ravi Kumar',kind:'user',content:'Truck is ready',state:'sent',at:2,edited_at:3},
  {id:'gone',chat:'orders',author:'ravi',name:'Ravi Kumar',kind:'user',content:'',state:'deleted',at:4}];
const response=(data,status=200)=>({ok:status<400,status,json:async()=>data});
async function show(render,list=messages,people=directory){
  globalThis.IntersectionObserver=class{observe(){}unobserve(){}disconnect(){}};
  await(await import('../src/workspace-context.ts')).enterWorkspace();
  const {ChatMessage}=await import('../src/react/chat/message.tsx');
  await render(()=>createElement(Fragment,null,list.map(message=>createElement(ChatMessage,{key:message.id,message,directory:people,openThread(){}}))));
}
function server(writes,{hold}={}){
  return async(path,options={})=>{
    if(options.method==='POST'){writes.push({path:path.replace(`/api/identity/workspaces/${w}`,''),body:JSON.parse(options.body)});return hold?new Promise(resolve=>{hold.release=()=>resolve(response({ok:true}));}):response({ok:true});}
    if(path.endsWith('/session'))return response(session);
    if(path.endsWith('/chat-directory'))return response(directory);
    if(path.endsWith('/chats'))return response({chats:[orders],preferences:{available:true,revision:0,pins:[]}});
    if(path.endsWith('/unread'))return response({unread:[]});
    if(path.includes('/chats/orders/messages'))return response({messages,agents:[]});
    return response({});
  };
}
const article=id=>document.querySelector(`[data-message-id="${id}"]`);
const inside=(node,label)=>[...node.querySelectorAll('button')].find(b=>b.textContent===label||b.getAttribute('aria-label')===label);

test('only your own messages offer Edit and Delete; deleted and edited states are visible',async()=>{
  await reactFixture(async({render,click,update})=>{
    await show(render);
    assert.equal(article('gone').textContent.includes('Message deleted'),true);
    assert.equal(article('gone').querySelector('[role=toolbar]'),null,'nothing to act on');
    assert.match(article('theirs').textContent,/edited/);
    await click(inside(article('theirs'),'More actions'));
    assert.equal(inside(article('theirs'),'Edit message'),undefined);
    assert.equal(inside(article('theirs'),'Delete message'),undefined);
    await click(inside(article('mine'),'More actions'));
    assert.ok(inside(article('mine'),'Edit message'));
    assert.ok(inside(article('mine'),'Delete message'));
  },{url,fetch:server([])});
});

test('editing submits once and deleting asks before writing',async()=>{
  const writes=[];const hold={};
  await reactFixture(async({render,click,update,change,submit})=>{
    await show(render);
    await click(inside(article('mine'),'More actions'));
    await click(inside(article('mine'),'Edit message'));
    const box=article('mine').querySelector('textarea[aria-label="Edit message"]');
    await change(box,'Dispatch at 1 pm');
    const form=article('mine').querySelector('form.chat-message-edit');
    await update(()=>{form.dispatchEvent(new window.Event('submit',{bubbles:true,cancelable:true}));form.dispatchEvent(new window.Event('submit',{bubbles:true,cancelable:true}));});
    assert.deepEqual(writes,[{path:'/chats/orders/messages/mine/edit',body:{content:'Dispatch at 1 pm'}}],'one request despite a double submit');
    await update(()=>hold.release());
    assert.equal(article('mine').querySelector('textarea'),null,'editing closes after saving');
    await click(inside(article('mine'),'More actions'));
    await click(inside(article('mine'),'Delete message'));
    const dialog=[...document.querySelectorAll('dialog')].find(d=>d.textContent.includes('Delete message?'));
    assert.match(dialog.textContent,/that work stops/);
    assert.equal(writes.length,1,'opening the confirmation writes nothing');
    await click(inside(dialog,'Delete'));
    await update(()=>hold.release());
    assert.deepEqual(writes[1],{path:'/chats/orders/messages/mine/delete',body:{}});
  },{url,fetch:server(writes,{hold})});
});

test('reaction chips toggle your own reaction and quick reactions add one',async()=>{
  const writes=[];
  await reactFixture(async({render,click,update})=>{
    await show(render);
    const chip=article('mine').querySelector('.chat-reaction');
    assert.equal(chip.getAttribute('aria-pressed'),'true');
    await click(chip);
    await click(inside(article('theirs'),'React ✅'));
    assert.deepEqual(writes.map(write=>write.body),[{emoji:'👍',on:false},{emoji:'✅',on:true}]);
    assert.ok(writes.every(write=>write.path.endsWith('/reactions')),'reactions never send messages or start work');
  },{url,fetch:server(writes)});
});

test('agent reactions show each agent by name and face, while people keep their own chips and picker',async()=>{
  const writes=[];
  const people={...directory,agents:[{id:'atlas',name:'Atlas',avatar:'robot-07'}]};
  // An emoji-only message: Atlas is in the directory, Vega only named by the server.
  const nod={id:'nod',chat:'orders',author:'meera',name:'Meera Iyer',kind:'user',content:'👍',state:'sent',at:5,delivered:2,seen:2,
    reactions:[{emoji:'🎉',count:1,mine:false}],agent_reactions:[{agent:'atlas',emoji:'👍',name:'Atlas'},{agent:'vega',emoji:'🙏',name:'Vega'}]};
  await reactFixture(async({render,click})=>{
    await show(render,[nod],people);
    const chips=[...article('nod').querySelectorAll('.chat-agent-reaction')];
    assert.deepEqual(chips.map(chip=>chip.textContent),['Atlas reacted 👍','Vega reacted 🙏']);
    assert.deepEqual(chips.map(chip=>chip.getAttribute('title')),['Atlas reacted 👍','Vega reacted 🙏']);
    assert.equal(chips[0].querySelector('img').getAttribute('src'),'/agent-avatars/robot-07.svg','the agent\'s own face');
    assert.match(chips[1].querySelector('img').getAttribute('src'),/^\/agent-avatars\/robot-\d\d\.svg$/);
    assert.equal(chips.some(chip=>chip.closest('button')!==null),false,'an agent\'s reaction is not a toggle');
    assert.equal(article('nod').querySelectorAll('button.chat-reaction').length,1,'people\'s chips are unchanged');
    await click(inside(article('nod'),'Add reaction'));
    assert.equal(article('nod').querySelectorAll('[aria-label="Choose a reaction"] button').length,15,'three words and the twelve emoji');
  },{url,fetch:server(writes)});
  assert.equal(writes.length,0,'showing agent reactions writes nothing');
});

test('messages that came by WhatsApp or email name their door; Chat messages do not (E3)',async()=>{
  const list=[
    {id:'wa',chat:'orders',author:'meera',name:'Meera Iyer',kind:'user',content:'Sent from my phone',state:'sent',at:1,door:'whatsapp'},
    {id:'mail',chat:'orders',author:'ravi',name:'Ravi Kumar',kind:'user',content:'Sent by email',state:'sent',at:2,door:'email'},
    {id:'typed',chat:'orders',author:'meera',name:'Meera Iyer',kind:'user',content:'Typed in Chat',state:'sent',at:3}];
  await reactFixture(async({render})=>{
    await show(render,list);
    assert.match(article('wa').textContent,/via WhatsApp/);
    assert.match(article('mail').textContent,/via email/);
    assert.equal(article('typed').textContent.includes('via '),false);
  },{url,fetch:server([])});
});

test('a reaction says who chose it, and words like "On it" react without a message',async()=>{
  const writes=[];
  await reactFixture(async({render,click})=>{
    const list=[{id:'plan',chat:'orders',author:'ravi',name:'Ravi Kumar',kind:'user',content:'Loading at 4',state:'sent',at:5,
      reactions:[{emoji:'👍',count:5,mine:true,users:['ravi','meera','gone','ravi2']},{emoji:'Done',count:1,mine:false,users:['ravi']}]}];
    await show(render,list);
    const chips=[...article('plan').querySelectorAll('button.chat-reaction')];
    assert.equal(chips[0].getAttribute('title'),'Ravi Kumar, You, A former member and 2 others reacted 👍');
    assert.equal(chips[0].getAttribute('aria-label'),'Ravi Kumar, You, A former member and 2 others reacted 👍. Remove yours');
    assert.equal(chips[1].getAttribute('title'),'Ravi Kumar reacted Done');
    assert.ok(chips[1].classList.contains('chat-reaction-word'));
    assert.equal(chips[1].textContent,'Done 1');
    await click(chips[1]);
    await click(inside(article('plan').querySelector('.chat-reactions'),'Add reaction'));
    const words=[...article('plan').querySelectorAll('[aria-label="Choose a reaction"] .chat-reaction-menu-word')].map(b=>b.textContent);
    assert.deepEqual(words,['On it','Done','Seen']);
    await click(inside(article('plan').querySelector('[aria-label="Choose a reaction"]'),'On it'));
    assert.deepEqual(writes.map(write=>write.body),[{emoji:'Done',on:true},{emoji:'On it',on:true}]);
  },{url,fetch:server(writes)});
});

test('who reacted reads well for one, two and many people', async () => {
  await reactFixture(async () => {
    await(await import('../src/workspace-context.ts')).enterWorkspace();
    const { whoReacted } = await import('../src/react/chat/message-actions.tsx');
    assert.equal(whoReacted({ emoji: '🎉', count: 2, mine: false, users: ['ravi', 'meera'] }, directory), 'Ravi Kumar and You reacted 🎉');
    assert.equal(whoReacted({ emoji: '🎉', count: 12, mine: false, users: ['ravi'] }, directory), 'Ravi Kumar and 11 others reacted 🎉');
    assert.equal(whoReacted({ emoji: '🎉', count: 3, mine: false }, directory), '3 people reacted 🎉', 'an older server sends counts only');
  }, { url, fetch: server([]) });
});

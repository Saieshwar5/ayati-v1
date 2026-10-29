import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement, Fragment } from 'react';
import { reactFixture } from './react-fixture.mjs';
const w='11111111-1111-4111-8111-111111111111',url=`http://localhost/w/${w}/?view=chat&conversation=orders`;
const session={user:{id:'meera',email:'meera@example.test'},workspaces:[{id:w,name:'Studio',role:'User'}]};
const directory={role:'User',users:[{id:'meera',name:'Meera Iyer',title:'',photo:null},{id:'ravi',name:'Ravi Kumar',title:'',photo:null}],agents:[]};
const orders={id:'orders',name:'orders',kind:'channel',visibility:'public',users:['meera','ravi'],agents:[],description:'',can_archive:true};
const messages=[
  {id:'mine',chat:'orders',author:'meera',name:'Meera Iyer',kind:'user',content:'Dispatch at noon',state:'sent',at:1,reactions:[{emoji:'👍',count:2,mine:true}]},
  {id:'theirs',chat:'orders',author:'ravi',name:'Ravi Kumar',kind:'user',content:'Truck is ready',state:'sent',at:2,edited_at:3},
  {id:'gone',chat:'orders',author:'ravi',name:'Ravi Kumar',kind:'user',content:'',state:'deleted',at:4}];
const response=(data,status=200)=>({ok:status<400,status,json:async()=>data});
async function show(render,list=messages){
  globalThis.IntersectionObserver=class{observe(){}unobserve(){}disconnect(){}};
  await(await import('../src/workspace-context.ts')).enterWorkspace();
  const {ChatMessage}=await import('../src/react/chat/message.tsx');
  await render(()=>createElement(Fragment,null,list.map(message=>createElement(ChatMessage,{key:message.id,message,directory,openThread(){}}))));
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

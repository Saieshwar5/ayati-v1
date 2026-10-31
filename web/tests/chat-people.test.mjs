import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';
const w='11111111-1111-4111-8111-111111111111',url=`http://localhost/w/${w}/?view=chat`;
const session={user:{id:'meera',email:'meera@example.test'},workspaces:[{id:w,name:'Studio',role:'Member'}]};
const directory={role:'Member',users:[
  {id:'meera',name:'Meera Iyer',title:'Design lead',about:'',photo:null,role:'Member'},
  {id:'ravi',name:'Ravi Kumar',title:'Operations',about:'Dispatch and suppliers.',photo:'33333333-3333-4333-8333-333333333333',role:'Member'},
  {id:'priya',name:'Priya Shah',title:'',about:'',photo:null,role:'Admin'}],
  agents:[{id:'atlas',name:'Atlas',role:'Orders assistant',audience:'selected',avatar:'robot-03',manager:'Priya Shah'},
    {id:'nova',name:'Nova',role:'Research',audience:'only_me',avatar:'spark',manager:'Meera Iyer'}]};
const orders={id:'orders',name:'orders',kind:'channel',visibility:'public',users:['meera','ravi'],agents:['atlas'],description:'',can_archive:true};
const response=(data,status=200)=>({ok:status<400,status,json:async()=>data});
async function start(){await(await import('../src/workspace-context.ts')).enterWorkspace();}
function fetcher(writes){return async(path,options={})=>{
  if(options.method==='POST'){writes.push({path,body:JSON.parse(options.body)});return response(path.endsWith('/chats')?{id:'new-dm'}:{ok:true});}
  return response(path.endsWith('/session')?session:path.endsWith('/chat-directory')?directory:path.endsWith('/chats')?{chats:[orders],preferences:{available:true,revision:0,pins:[]}}:path.endsWith('/unread')?{unread:[{chat:'orders',count:3}]}:{});
};}

test('everyone in the workspace and every usable agent appear together, agents marked AI',async()=>{
  const writes=[];
  await reactFixture(async({render,button,click,change})=>{
    await start();const {WorkspaceChat}=await import('../src/react/chat/workspace-chat.tsx');
    await render(WorkspaceChat,{href:url});
    // Unread channels stand out and agent pictures are round robots or uploads.
    assert.ok(document.querySelector('a.chat-contact.chat-contact-unread'));
    await click(button('People & agents3 · 2'));
    const names=[...document.querySelectorAll('dialog li .chat-name-text')].map(node=>node.textContent);
    assert.deepEqual(names,['Atlas','Meera Iyer (you)','Nova','Priya Shah','Ravi Kumar']);
    const atlas=[...document.querySelectorAll('dialog li')].find(li=>li.textContent.includes('Atlas'));
    assert.equal(atlas.querySelector('.ai-tag').textContent,'AI');
    assert.equal(atlas.querySelector('img').getAttribute('src'),'/agent-avatars/robot-03.svg');
    const nova=[...document.querySelectorAll('dialog li')].find(li=>li.textContent.includes('Nova'));
    assert.match(nova.querySelector('img').getAttribute('src'),/^\/agent-avatars\/robot-\d\d\.svg$/,'old symbols become robots');
    const ravi=[...document.querySelectorAll('dialog li')].find(li=>li.textContent.includes('Ravi'));
    assert.equal(ravi.querySelector('img').getAttribute('src'),`/api/identity/workspaces/${w}/members/ravi/photo/33333333-3333-4333-8333-333333333333`);
    // Nobody can message or pin themselves.
    const self=[...document.querySelectorAll('dialog li')].find(li=>li.textContent.includes('(you)'));
    assert.equal(self.querySelectorAll('button').length,1);
    await change(document.querySelector('dialog input[type=search]'),'oper');
    assert.deepEqual([...document.querySelectorAll('dialog li .chat-name-text')].map(n=>n.textContent),['Ravi Kumar']);
    await click(button('Agents 2'));
    assert.deepEqual([...document.querySelectorAll('dialog li')].length,0);
    assert.equal(writes.length,0,'browsing never writes');
  },{url,fetch:fetcher(writes)});
});

test('a profile card opens from the directory and Message starts one direct conversation',async()=>{
  const writes=[];
  await reactFixture(async({render,button,click})=>{
    await start();const {WorkspaceChat}=await import('../src/react/chat/workspace-chat.tsx');
    await render(WorkspaceChat,{href:url});
    await click(button('People & agents3 · 2'));
    await click(button('View profile of Ravi Kumar'));
    const card=[...document.querySelectorAll('dialog')].find(d=>d.textContent.includes('Dispatch and suppliers.'));
    assert.ok(card,'about text shown');
    assert.match(card.textContent,/Channels together# orders/);
    const message=[...card.querySelectorAll('button')].find(b=>b.textContent==='Message');
    await click(message);
    assert.equal(writes.length,1);
    assert.equal(writes[0].body.kind,'direct');
    assert.deepEqual(writes[0].body.users,['ravi']);
  },{url,fetch:fetcher(writes)});
});

test('an agent card shows who manages it and adds it to a channel the viewer manages',async()=>{
  const writes=[];
  const second={...orders,id:'design',name:'design',agents:[]};
  await reactFixture(async({render,button,click,change,submit})=>{
    await start();const {WorkspaceChat}=await import('../src/react/chat/workspace-chat.tsx');
    await render(WorkspaceChat,{href:url});
    await click(button('People & agents3 · 2'));
    await click(button('View profile of Atlas'));
    const card=[...document.querySelectorAll('dialog')].find(d=>d.textContent.includes('Managed by'));
    assert.match(card.textContent,/Priya Shah/);
    assert.match(card.textContent,/is an AI agent/);
    const select=card.querySelector('select');
    assert.deepEqual([...select.options].map(o=>o.textContent),['Choose a channel…','#design'],'channels it already works in are not offered');
    await change(select,'design');
    await submit(card.querySelector('form.profile-card-add'));
    assert.deepEqual(writes,[{path:`/api/identity/workspaces/${w}/chats/design/agents`,body:{agents:['atlas']}}]);
  },{url,fetch:async(path,options={})=>{
    if(options.method==='POST'){writes.push({path,body:JSON.parse(options.body)});return response({ok:true});}
    return response(path.endsWith('/session')?session:path.endsWith('/chat-directory')?directory:path.endsWith('/chats')?{chats:[orders,second],preferences:{available:true,revision:0,pins:[]}}:path.endsWith('/unread')?{unread:[]}:{});
  }});
});

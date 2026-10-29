import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';
const w='11111111-1111-4111-8111-111111111111',url=`http://localhost/w/${w}/?view=chat`;
const session={user:{id:'meera',email:'meera@example.test'},workspaces:[{id:w,name:'Studio',role:'User'}]};
const directory={role:'User',users:[{id:'meera',name:'Meera Iyer'},{id:'ravi',name:'Ravi Kumar'}],agents:[]};
const orders={id:'orders',name:'orders',kind:'channel',visibility:'public',users:['meera','ravi'],agents:[],description:''};
const quiet={id:'quiet',name:'quiet',kind:'channel',visibility:'public',users:['meera','ravi'],agents:[],description:''};
const direct={id:'dm',name:'dm',kind:'direct',users:['meera','ravi'],agents:[],description:''};
const response=(data,status=200)=>({ok:status<400,status,json:async()=>data});
async function start(){await(await import('../src/workspace-context.ts')).enterWorkspace();}

test('alerts follow each level: all, @mentions and replies, or nothing',async()=>{
  await reactFixture(async()=>{
    await start();
    const { shouldAlert, alertText, defaultLevel }=await import('../src/react/chat/notifications.ts');
    const now=1000, base={id:'m',chat:'orders',author:'ravi',name:'Ravi',kind:'user',content:'Hi',state:'sent',at:now};
    assert.equal(defaultLevel(direct),'all');
    assert.equal(defaultLevel(orders),'mentions');
    assert.equal(shouldAlert(base,orders,'all',false,now),true);
    assert.equal(shouldAlert(base,orders,'mentions',false,now),false,'channel chatter stays quiet by default');
    assert.equal(shouldAlert({...base,for_me:true},orders,'mentions',false,now),true);
    assert.equal(shouldAlert({...base,for_me:true},orders,'none',false,now),false,'muted means nothing');
    assert.equal(shouldAlert({...base,for_me:true},orders,'all',true,now),false,'not while reading it');
    assert.equal(shouldAlert({...base,author:'meera'},orders,'all',false,now),false,'never your own message');
    assert.equal(shouldAlert({...base,at:now-600},orders,'all',false,now),false,'no alerts for old history');
    assert.equal(shouldAlert({...base,kind:'agent',state:'queued',for_me:true},orders,'all',false,now),false,'agents alert once finished');
    assert.equal(shouldAlert({...base,kind:'agent',state:'complete',for_me:true},orders,'mentions',false,now),true);
    assert.equal(alertText('[@Meera Iyer](mention:user:meera) please **check** the `stock`'),'@Meera Iyer please check the stock');
  },{url,fetch:async path=>response(path.endsWith('/session')?session:{})});
});

test('the sidebar marks counts that are for you and muted conversations; the bell saves a level once',async()=>{
  const writes=[];let finish;
  await reactFixture(async({render,click,update})=>{
    await start();const {WorkspaceChat}=await import('../src/react/chat/workspace-chat.tsx');
    await render(WorkspaceChat,{href:`${url}&conversation=orders`});
    const row=name=>[...document.querySelectorAll('a.chat-contact')].find(a=>a.textContent.includes(name));
    assert.ok(row('orders').querySelector('.chat-unread.chat-unread-mention'),'a mention makes the count stand out');
    assert.equal(row('orders').querySelector('.chat-unread').getAttribute('aria-label'),'4 unread messages, 1 for you');
    assert.ok(row('quiet').querySelector('.chat-muted'),'muted conversations show a crossed bell');
    const bell=document.querySelector('.chat-notify-button');
    assert.equal(bell.getAttribute('aria-label'),'Notifications: @mentions and replies');
    await click(bell);
    const nothing=[...document.querySelectorAll('[role=menuitemradio]')].find(b=>b.textContent.startsWith('Nothing'));
    await update(()=>{nothing.click();nothing.click();});
    assert.deepEqual(writes,[{path:`/api/identity/workspaces/${w}/chats/orders/notifications`,body:{level:'none'}}]);
    await update(()=>finish(response({ok:true,level:'none'})));
    assert.equal(document.querySelector('.chat-notify-button').getAttribute('aria-label'),'Notifications: Nothing');
    assert.ok(row('orders').querySelector('.chat-muted'));
  },{url:`${url}&conversation=orders`,fetch:async(path,options={})=>{
    if(options.method==='POST'){writes.push({path,body:JSON.parse(options.body)});return new Promise(resolve=>{finish=resolve;});}
    return response(path.endsWith('/session')?session:path.endsWith('/chat-directory')?directory:path.endsWith('/chats')?{chats:[orders,quiet,direct],preferences:{available:true,revision:0,pins:[]}}
      :path.endsWith('/chat-notifications')?{levels:{quiet:'none'}}:path.endsWith('/unread')?{unread:[{chat:'orders',count:4,mentions:1}]}:path.includes('/messages')?{messages:[],agents:[]}:{});
  }});
});

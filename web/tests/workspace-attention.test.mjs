import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { reactFixture } from './react-fixture.mjs';
const w='11111111-1111-4111-8111-111111111111',url=`http://localhost/w/${w}/?view=home`;
const session={user:{id:'requester',email:'requester@example.test'},workspaces:[{id:w,name:'Store',role:'Owner'}]};
const item={kind:'question',id:'q',version:'pending',title:'Choose the customer group',status:'pending',at:1000,agent:'a',agent_name:'Nova',chat:'source',thread:'thread',reply:'reply',conversation_name:'Customer care',archived:false,read:false,reviewed:false};
const result={...item,kind:'outcome',id:'result',version:'failed:1000',title:'Monthly report',status:'failed'};
const response=(data,status=200)=>({ok:status<400,status,json:async()=>data});
const page=(items=[item],next_before=null)=>response({available:true,items,next_before});
async function setup(){const context=await import('../src/workspace-context.ts');await context.enterWorkspace();return (await import('../src/react/attention/inbox.tsx')).AttentionInbox;}

test('attention opens an exact question through a personal read receipt without submitting an answer',async()=>{
  const writes=[];
  await reactFixture(async({render,button,click})=>{
    const Inbox=await setup();await render(Inbox,{close:()=>{}});assert.equal(writes.length,0);
    assert.match(document.body.textContent,/Unread/);
    await click(button('Answer question'));
    assert.deepEqual(writes,[{kind:'question',id:'q',version:'pending',reviewed:false}]);
    const questionDialog=[...document.querySelectorAll('dialog')].find(node=>node.querySelector('h2').textContent==='Saved question');
    assert.ok(questionDialog);
    await click(questionDialog.querySelector('[aria-label="Close"]'));
    assert.equal(document.activeElement,button('Answer question'));
    assert.match(document.body.textContent,/Choose the customer group/);
    assert.equal(writes.length,1);
    const source=document.querySelector('.attention-row-actions a');
    assert.match(source.href,/conversation=source&message=reply&thread=thread/);
  },{url,fetch:async(path,options={})=>{
    if(path.endsWith('/session'))return response(session);
    if(options.method==='POST'){writes.push(JSON.parse(options.body));return response({saved:true});}
    if(path.includes('/questions/'))return response({id:'q',revision:1,state:'pending',specification:{version:1,kind:'single_choice',title:'Saved question',description:'',fields:[],options:[{id:'a',label:'Group A'}]},respondent:'requester',respondent_name:'Ananya',can_respond:true,reply:'reply',chat:'source'});
    return page([{...item,read:writes.length>0}]);
  }});
});

test('only explicit failed-outcome review removes the item and double clicks submit once',async()=>{
  const writes=[];let finish,reviewed=false;
  await reactFixture(async({render,button,update,click})=>{
    const Inbox=await setup();await render(Inbox,{close:()=>{}});assert.equal(writes.length,0);
    await update(()=>{const node=button('Mark outcome reviewed');node.click();node.click();});
    assert.equal(writes.length,1);assert.equal(writes[0].reviewed,true);
    await update(()=>{reviewed=true;finish(response({saved:true}));});
    assert.equal(document.querySelector('.attention-row'),null);
    await click(button('Recent work'));assert.match(document.body.textContent,/Reviewed/);
    assert.equal(button('Mark outcome reviewed'),undefined);
  },{url,fetch:async(path,options={})=>{
    if(path.endsWith('/session'))return response(session);
    if(options.method==='POST'){writes.push(JSON.parse(options.body));return new Promise(resolve=>{finish=resolve;});}
    return path.includes('view=recent')?page([{...result,read:true,reviewed:true}]):page(reviewed?[]:[result]);
  }});
});

test('failed read receipts show the source error and never open an approval or navigate',async()=>{
  let denied=false;const writes=[];
  await reactFixture(async({render,button,click,navigation})=>{
    const Inbox=await setup();await render(Inbox,{close:()=>{}});
    await click(button('Review action'));
    assert.deepEqual(writes,[{kind:'approval',id:'q',version:'pending',reviewed:false}]);
    assert.match(document.body.textContent,/Source no longer available/);
    assert.equal(document.body.textContent.includes('Choose the customer group'),false);
    assert.equal(document.querySelector('.approval-detail'),null);assert.deepEqual(navigation,[]);
  },{url,fetch:async(path,options={})=>{
    if(path.endsWith('/session'))return response(session);
    if(options.method==='POST'){writes.push(JSON.parse(options.body));denied=true;return response({error:'Source no longer available'},404);}
    return denied?response({error:'Source no longer available'},404):page([{...item,kind:'approval'}]);
  }});
});

test('older pages are bounded, tabs support keyboard, and late responses cannot replace another view',async()=>{
  const pending=[];let slow=false;
  await reactFixture(async({render,button,click,update})=>{
    const Inbox=await setup();await render(Inbox,{close:()=>{}});
    await click(button('Older items'));assert.match(document.body.textContent,/Older question/);
    await click(button('Newer items'));assert.match(document.body.textContent,/Choose the customer group/);
    await click(button('Older items'));await click(button('Refresh attention'));
    assert.match(document.body.textContent,/Choose the customer group/);assert.equal(button('Newer items'),undefined);
    slow=true;await click(button('Refresh attention'));
    await update(()=>button('Needs you').dispatchEvent(new window.KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true})));
    assert.equal(button('Recent work').getAttribute('aria-selected'),'true');assert.equal(document.activeElement,button('Recent work'));
    assert.match(document.body.textContent,/Monthly report/);
    await update(()=>pending.forEach(resolve=>resolve(page([{...item,title:'STALE_PRIVATE_QUESTION'}]))));
    assert.equal(document.body.textContent.includes('STALE_PRIVATE_QUESTION'),false);
    assert.match(document.body.textContent,/Monthly report/);
  },{url,fetch:async path=>{
    if(path.endsWith('/session'))return response(session);
    if(path.includes('view=recent'))return page([result]);
    if(slow)return new Promise(resolve=>pending.push(resolve));
    return path.includes('before=')?page([{...item,id:'older',title:'Older question'}]):page([item],'cursor');
  }});
});

test('late read acknowledgments after unmount cannot navigate to an old identity source',async()=>{
  let finish;
  await reactFixture(async({render,button,click,update,navigation})=>{
    const Inbox=await setup();await render(Inbox,{close:()=>{}});
    await click(button('Open work'));
    await render(()=>createElement('p',null,'Different workspace'));
    await update(()=>finish(response({saved:true})));
    assert.deepEqual(navigation,[]);assert.match(document.body.textContent,/Different workspace/);
  },{url,fetch:async(path,options={})=>path.endsWith('/session')?response(session):options.method==='POST'?new Promise(resolve=>{finish=resolve;}):page([result])});
});

test('Home refreshes on focus and aborts owned reads when its view unmounts',async()=>{
  const signals=[];let delay=false;
  await reactFixture(async({render,update})=>{
    const context=await import('../src/workspace-context.ts');await context.enterWorkspace();
    const {HomeAttention}=await import('../src/react/attention/home-attention.tsx');
    await render(HomeAttention,{privateData:{items:[],total:0,available:false},openPrivate:()=>{}});
    assert.match(document.body.textContent,/Choose the customer group/);
    delay=true;
    await update(()=>window.dispatchEvent(new window.Event('focus')));
    assert.ok(signals.some(signal=>signal));
    await render(()=>createElement('p',null,'Closed Home'));
    assert.ok(signals.at(-1).aborted);
  },{url,fetch:async(path,options={})=>{
    if(path.endsWith('/session'))return response(session);
    if(path.includes('message-capabilities'))return response({live:false});
    if(path.includes('/attention')){signals.push(options.signal);return delay?new Promise(()=>{}):page();}
    return response({});
  }});
});

test('Home shows quiet recent results without inventing a total',async()=>{
  await reactFixture(async({render,button,click})=>{
    await setup();const {HomeAttention}=await import('../src/react/attention/home-attention.tsx');
    await render(HomeAttention);
    assert.equal(button('View attention · 1+').textContent,'View attention · 1+');
    await click(button('View attention · 1+'));assert.equal(document.querySelector('dialog h2').textContent,'Your attention');
    await click(button('Recent work'));assert.match(document.body.textContent,/Monthly report/);
  },{url,fetch:async path=>path.endsWith('/session')?response(session):path.includes('view=recent')?page([result]):page([item],'more')});
});

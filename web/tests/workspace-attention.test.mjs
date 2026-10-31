import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createElement } from 'react';
import { reactFixture } from './react-fixture.mjs';
const w='11111111-1111-4111-8111-111111111111',url=`http://localhost/w/${w}/?view=activity`;
const session={user:{id:'requester',email:'requester@example.test'},workspaces:[{id:w,name:'Store',role:'Admin'}]};
const item={kind:'question',id:'q',version:'pending',title:'Choose the customer group',status:'pending',at:1000,agent:'a',agent_name:'Nova',chat:'source',thread:'thread',reply:'reply',conversation_name:'Customer care',archived:false,read:false,reviewed:false};
const result={...item,kind:'outcome',id:'result',version:'failed:1000',title:'Monthly report',status:'failed'};
const response=(data,status=200)=>({ok:status<400,status,json:async()=>data});
const page=(items=[item],next_before=null)=>response({available:true,items,next_before});
async function setup(){const context=await import('../src/workspace-context.ts');await context.enterWorkspace();return (await import('../src/react/activity/activity-page.tsx')).ActivityPage;}
const render=async(fixture,Page)=>fixture.render(Page,{href:url});

test('Needs me opens a question only after its read receipt and never answers it',async()=>{
  const writes=[];
  await reactFixture(async fixture=>{
    const {button,click}=fixture;
    await render(fixture,await setup());assert.equal(writes.length,0);
    assert.equal(button('Needs me1').getAttribute('aria-selected'),'true');
    assert.match(document.body.textContent,/Choose the customer group.*new/);
    await click(button('Answer'));
    assert.deepEqual(writes,[{kind:'question',id:'q',version:'pending',reviewed:false}]);
    const questionDialog=[...document.querySelectorAll('dialog')].find(node=>node.querySelector('h2').textContent==='Saved question');
    assert.ok(questionDialog);
    await click(questionDialog.querySelector('[aria-label="Close"]'));
    assert.equal(writes.length,1);
  },{url,fetch:async(path,options={})=>{
    if(path.endsWith('/session'))return response(session);
    if(options.method==='POST'){writes.push(JSON.parse(options.body));return response({saved:true});}
    if(path.includes('/questions/'))return response({id:'q',revision:1,state:'pending',specification:{version:1,kind:'single_choice',title:'Saved question',description:'',fields:[],options:[{id:'a',label:'Group A'}]},respondent:'requester',respondent_name:'Ananya',can_respond:true,reply:'reply',chat:'source'});
    return page([{...item,read:writes.length>0}]);
  }});
});

test('only an explicit review clears a failed outcome and a double click sends once',async()=>{
  const writes=[];let finish,reviewed=false;
  await reactFixture(async fixture=>{
    const {button,update}=fixture;
    await render(fixture,await setup());assert.equal(writes.length,0);
    await update(()=>{const node=button('Mark reviewed');node.click();node.click();});
    assert.equal(writes.length,1);assert.equal(writes[0].reviewed,true);
    await update(()=>{reviewed=true;finish(response({saved:true}));});
    assert.match(document.body.textContent,/Nothing needs you right now/);
    assert.equal(button('Mark reviewed'),undefined);
  },{url,fetch:async(path,options={})=>{
    if(path.endsWith('/session'))return response(session);
    if(options.method==='POST'){writes.push(JSON.parse(options.body));return new Promise(resolve=>{finish=resolve;});}
    return page(reviewed?[]:[result]);
  }});
});

test('a failed read receipt shows the error and opens nothing',async()=>{
  let denied=false;const writes=[];
  await reactFixture(async fixture=>{
    const {button,click,navigation}=fixture;
    await render(fixture,await setup());
    await click(button('Review'));
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

test('older pages are bounded, tabs follow the keyboard and the count shows more',async()=>{
  await reactFixture(async fixture=>{
    const {button,click,update}=fixture;
    await render(fixture,await setup());
    assert.ok(button('Needs me1+'));
    await click(button('Older'));assert.match(document.body.textContent,/Older question/);
    await click(button('Newer'));assert.match(document.body.textContent,/Choose the customer group/);
    await update(()=>button('Needs me1+').dispatchEvent(new window.KeyboardEvent('keydown',{key:'ArrowRight',bubbles:true})));
    assert.equal(button('Now').getAttribute('aria-selected'),'true');assert.equal(document.activeElement,button('Now'));
  },{url,fetch:async path=>{
    if(path.endsWith('/session'))return response(session);
    if(path.includes('/activity/now'))return response({agents:[],running:[],away:{since:0,totals:{requests:0}}});
    return path.includes('before=')?page([{...item,id:'older',title:'Older question'}]):page([item],'cursor');
  }});
});

test('a late receipt after leaving Activity cannot open an old item',async()=>{
  let finish;
  await reactFixture(async fixture=>{
    const {button,click,update,render:show}=fixture;
    await render(fixture,await setup());
    await click(button('Open'));
    await show(()=>createElement('p',null,'Different workspace'));
    await update(()=>finish(response({saved:true})));
    assert.match(document.body.textContent,/Different workspace/);
    assert.equal(document.querySelector('dialog'),null);
  },{url,fetch:async(path,options={})=>path.endsWith('/session')?response(session):options.method==='POST'?new Promise(resolve=>{finish=resolve;}):page([result])});
});

test('Home refreshes on focus, aborts owned reads when it unmounts and links to Activity',async()=>{
  const signals=[];let delay=false;
  await reactFixture(async({render,update})=>{
    const context=await import('../src/workspace-context.ts');await context.enterWorkspace();
    const {HomeAttention}=await import('../src/react/attention/home-attention.tsx');
    await render(HomeAttention);
    assert.match(document.body.textContent,/Choose the customer group/);
    const open=[...document.querySelectorAll('a')].find(node=>node.textContent==='Open Activity · 1+');
    assert.match(open.href,/view=activity&tab=needs/);
    delay=true;
    await update(()=>window.dispatchEvent(new window.Event('focus')));
    assert.ok(signals.some(signal=>signal));
    await render(()=>createElement('p',null,'Closed Home'));
    assert.ok(signals.at(-1).aborted);
  },{url:`http://localhost/w/${w}/?view=home`,fetch:async(path,options={})=>{
    if(path.endsWith('/session'))return response(session);
    if(path.includes('/attention')){signals.push(options.signal);return delay?new Promise(()=>{}):page([item],'more');}
    return response({});
  }});
});

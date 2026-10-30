import { registerHooks } from 'node:module';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format:'module', source:'export {};', shortCircuit:true } : next(url, context); } });
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reactFixture } from './react-fixture.mjs';
const saved = (body, state='complete') => ({ id:'reply',schema:1,body,version:body,sequence:1,state,long:true,available:true,bytes:body.length,characters:body.length,format:'markdown' });
const adapter = read => ({ read,href:path=>path,response:id=>`/messages/${id}/response`,file:id=>`/files/${id}` });

test('corner expansion reads only on demand, keeps the same saved body across stream updates and collapses in place',async()=>{
  let calls=0,changed;
  const source={...adapter(async()=>{calls++;return saved('EXACT_SAVED_FULL_BODY','draft');}),subscribe:fn=>{changed=fn;return()=>{changed=undefined;};}};
  await reactFixture(async({render,button,click})=>{
    const {InlineResponse}=await import('../src/react/results/inline-response.tsx');
    const props={id:'reply',text:'Preview '.repeat(200),source};
    await render(InlineResponse,props);
    assert.equal(calls,0);assert.equal(button('Expand response').getAttribute('aria-expanded'),'false');
    assert.equal(button('Expand response').textContent,'');
    await click(button('Expand response'));
    assert.match(document.body.textContent,/EXACT_SAVED_FULL_BODY/);
    assert.equal(document.querySelector('dialog'),null);
    const count=calls;
    await render(InlineResponse,{...props,text:'NEW_STREAM_TEXT '.repeat(200)});
    assert.match(document.body.textContent,/EXACT_SAVED_FULL_BODY/);assert.doesNotMatch(document.body.textContent,/NEW_STREAM_TEXT/);assert.equal(calls,count);
    await click(button('Collapse response'));
    assert.doesNotMatch(document.body.textContent,/EXACT_SAVED_FULL_BODY/);assert.match(document.body.textContent,/NEW_STREAM_TEXT/);assert.equal(changed,undefined);
  });
});

test('inline reading offers explicit updates and exact downloads, clears revoked source content and fences late reads',async()=>{
  let notify,revoked=false,hold=false,late,current=saved('PRIVATE_SAVED_BODY','draft');
  const source={...adapter(async path=>{
    if(revoked)throw Object.assign(new Error('Source access revoked'),{status:403});
    if(hold && path.includes('metadata=true'))return new Promise(resolve=>{late=resolve;});
    return {...current,source_scope:'source-a'};
  }),subscribe:fn=>{notify=fn;return()=>{notify=undefined;};}};
  await reactFixture(async({render,button,click,update})=>{
    const {InlineResponse}=await import('../src/react/results/inline-response.tsx');
    await render(InlineResponse,{id:'reply',text:'Long preview '.repeat(100),source});
    await click(button('Expand response'));
    current=saved('NEW_COMPLETE_BODY');await update(()=>notify());
    assert.match(document.body.textContent,/PRIVATE_SAVED_BODY/);assert.doesNotMatch(document.body.textContent,/NEW_COMPLETE_BODY/);
    await click(button('Load latest saved response'));
    assert.match(document.body.textContent,/NEW_COMPLETE_BODY/);
    assert.equal(new URL(document.querySelector('a[download]').href).searchParams.get('version'),'NEW_COMPLETE_BODY');
    hold=true;await update(()=>notify());revoked=true;await update(()=>notify('source-a'));
    assert.doesNotMatch(document.body.textContent,/PRIVATE_SAVED_BODY|NEW_COMPLETE_BODY/);
    await update(()=>late(saved('PRIVATE_LATE_BODY')));
    assert.doesNotMatch(document.body.textContent,/PRIVATE_LATE_BODY/);assert.ok(button('Retry'));
  });
  assert.equal(notify,undefined);
});

test('failed expansion can retry, source changes fence old bodies, and short answers have no expansion control',async()=>{
  let fail=true,finish;
  const source=adapter(async path=>{
    if(path.includes('/old/'))return new Promise(resolve=>{finish=resolve;});
    if(fail)throw new Error('Connection unavailable');
    return saved('CURRENT_SAVED_BODY');
  });
  await reactFixture(async({render,button,click,update})=>{
    const {InlineResponse}=await import('../src/react/results/inline-response.tsx');
    await render(InlineResponse,{id:'reply',text:'Short answer',source});assert.equal(button('Expand response'),undefined);
    const props={id:'reply',text:'Long text '.repeat(150),source};
    await render(InlineResponse,props);await click(button('Expand response'));assert.match(document.body.textContent,/Connection unavailable/);
    fail=false;await click(button('Retry'));assert.match(document.body.textContent,/CURRENT_SAVED_BODY/);
    await render(InlineResponse,{...props,id:'old'});await render(InlineResponse,{...props,id:'new'});
    await update(()=>finish(saved('PRIVATE_OLD_BODY')));
    assert.doesNotMatch(document.body.textContent,/PRIVATE_OLD_BODY/);assert.match(document.body.textContent,/CURRENT_SAVED_BODY/);
    await render(()=>null);
  });
});

test('Chat omits progress launchers while retaining reply cancellation and ordinary agent prose boxes',async()=>{
  await reactFixture(async({render,button,click})=>{
    const {ChatMessage}=await import('../src/react/chat/message.tsx');
    await render(ChatMessage,{message:{id:'reply',chat:'chat',kind:'agent',author:'agent',state:'queued',content:'Saved short reply',at:1,has_task:true,can_cancel:true},directory:{agents:[{id:'agent',name:'Suresh'}],users:[]}});
    assert.equal(button('Check progress'),undefined);await click(button('More actions'));assert.ok(button('Cancel request'));
    assert.match(document.querySelector('.chat-response').textContent,/Saved short reply/);
  });
});

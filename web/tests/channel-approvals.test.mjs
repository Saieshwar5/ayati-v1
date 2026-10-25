import { test } from 'node:test';
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
import { reactFixture } from './react-fixture.mjs';
registerHooks({ load(url, context, next) { return url.endsWith('.css') ? { format: 'module', source: 'export {};', shortCircuit: true } : next(url, context); } });
const workspace='11111111-1111-4111-8111-111111111111';
const url=`http://localhost/w/${workspace}/?view=chat`;
const approval={id:'action',agent:'agent',title:'Update address',agent_name:'Nova',account_name:'Company orders',approver_name:'Priya',requester_name:'Owner',conversation_name:'Support',arguments:{order:'one',address:'<script>private()</script>'},state:'pending',can_decide:true,chat:'source',thread:null};
const response=data=>({ok:true,status:200,json:async()=>data});

test('channel references disclose no exact fields and opening a request grants nothing',async()=>{
  const writes=[];
  await reactFixture(async({render,button,click})=>{
    const {ApprovalCard}=await import('../src/react/approvals/approval-card.tsx');
    await render(ApprovalCard,{reference:{id:'action',state:'pending'}});
    assert.equal(document.body.textContent.includes('private()'),false);assert.equal(writes.length,0);
    await click(button('Review saved request'));
    assert.match(document.querySelector('dl').textContent,/<script>private/);assert.equal(document.querySelector('script'),null);
    assert.equal(writes.length,0);
  },{url,fetch:async(_,options={})=>{if(options.body)writes.push(options.body);return response(approval);}});
});

test('restricted channel viewers cannot open details or infer the saved action identity',async()=>{
  let reads=0;
  await reactFixture(async({render,button})=>{
    const {ApprovalCard}=await import('../src/react/approvals/approval-card.tsx');
    await render(ApprovalCard,{reference:{restricted:true}});
    assert.equal(button('Review saved request'),undefined);assert.equal(document.querySelector('dl'),null);assert.equal(reads,0);
  },{url,fetch:async()=>{reads++;return response(approval);}});
});

test('only a server-authorized approver sees decision controls',async()=>{
  await reactFixture(async({render,button})=>{
    const {ApprovalDialog}=await import('../src/react/approvals/approval-dialog.tsx');
    await render(ApprovalDialog,{id:'action',close(){}});
    assert.equal(button('Approve this action'),undefined);assert.equal(button('Reject'),undefined);assert.equal(button('Request changes'),undefined);
    assert.match(document.body.textContent,/Waiting for the designated authorized approver/);
  },{url,fetch:async()=>response({...approval,can_decide:false})});
});

test('approval guards double submission and retries an uncertain response with the same decision identity',async()=>{
  const writes=[];let release;let fail=true;
  await reactFixture(async({render,button,update,click})=>{
    const {ApprovalDialog}=await import('../src/react/approvals/approval-dialog.tsx');
    await render(ApprovalDialog,{id:'action',close(){}});
    await update(()=>{button('Approve this action').click();button('Approve this action').click();});
    assert.equal(writes.length,1);await update(()=>release());assert.match(document.body.textContent,/Response unavailable/);
    fail=false;await click(button('Approve this action'));await update(()=>release());
    assert.deepEqual(writes[0],writes[1]);assert.match(writes[0].id,/^[a-f0-9-]{36}$/);assert.equal(writes[0].decision,'approve');
  },{url,fetch:async(_,options={})=>{
    if(options.body){writes.push(JSON.parse(options.body));await new Promise(resolve=>{release=resolve;});return fail?{ok:false,status:503,json:async()=>({error:'Response unavailable'})}:response(approval);}
    return response(approval);
  }});
});

test('request changes targets the same immutable action and access loss clears exact fields',async()=>{
  const writes=[];let deny=false;
  await reactFixture(async({render,button,click})=>{
    const {ApprovalDialog}=await import('../src/react/approvals/approval-dialog.tsx');
    await render(ApprovalDialog,{id:'action',close(){}});await click(button('Request changes'));
    assert.equal(writes[0].decision,'request_changes');assert.equal(writes[0].arguments,undefined);
    deny=true;await click(button('Refresh request'));
    assert.equal(document.querySelector('dl'),null);assert.equal(button('Approve this action'),undefined);assert.match(document.body.textContent,/Source access revoked/);
  },{url,fetch:async(_,options={})=>{
    if(options.body){writes.push(JSON.parse(options.body));return response({...approval,state:'rejected',decision:'request_changes',can_decide:false});}
    return deny?{ok:false,status:400,json:async()=>({error:'Source access revoked'})}:response(approval);
  }});
});

test('a late request cannot replace details for another action',async()=>{
  let release;
  await reactFixture(async({render,update})=>{
    const {ApprovalDialog}=await import('../src/react/approvals/approval-dialog.tsx');
    await render(ApprovalDialog,{key:'old',id:'old',close(){}});await render(ApprovalDialog,{key:'new',id:'new',close(){}});await update(()=>release());
    assert.match(document.body.textContent,/Current request/);assert.equal(document.body.textContent.includes('private()'),false);
  },{url,fetch:async path=>{
    if(path.endsWith('/old')){await new Promise(resolve=>{release=resolve;});return response(approval);}
    return response({...approval,title:'Current request',arguments:{order:'new'}});
  }});
});

test('routing selections bind the authorized person revision and chosen delivery channel',async()=>{
  let route={};
  await reactFixture(async({render,change})=>{
    const {ApprovalRoute}=await import('../src/react/approvals/approval-route.tsx');
    const props={label:'Update order',people:[{id:'priya',name:'Priya',revision:7}],channels:[{id:'channel',name:'Approvals'}],change:value=>{route=value;}};
    await render(ApprovalRoute,{...props,route});
    await change(document.querySelector('[aria-label="Update order approver"]'),'priya');
    assert.deepEqual(route,{approver:'priya',approver_revision:7});
    await render(ApprovalRoute,{...props,route});await change(document.querySelector('[aria-label="Update order approval channel"]'),'channel');
    assert.equal(route.approval_chat,'channel');assert.equal(route.approver,'priya');
  },{url});
});

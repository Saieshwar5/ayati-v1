import {test} from 'node:test';
import assert from 'node:assert/strict';
import {reactFixture} from './react-fixture.mjs';
const workspace='11111111-1111-4111-8111-111111111111';
const url=`http://localhost/w/${workspace}/?view=home`;
const session={user:{id:'owner',email:'owner@example.test'},workspaces:[{id:workspace,name:'Store',role:'Owner'}]};
const response=(value,status=200)=>({ok:status<400,status,json:async()=>value});
const panel={id:'00000000-0000-4000-8000-000000000001',revision:1,active_revision:1,agent_name:'Mira',
  active:{available:true,definition:{title:'Personal requests'}},
  maintenance:{revision:0,panel_revision:1,enabled:false,eligible:true,execution_available:true,
    instructions:'',interval_seconds:null,stale_seconds:3600,scopes:[{connection:'support',operation:'request.read',arguments:{assigned:'me'}}],
    status:'paused',checked_ms:null,next_ms:null,saved_ms:null}};
async function setup(){const context=await import('../src/workspace-context.ts');await context.enterWorkspace();return import('../src/react/home-panels/maintenance.tsx');}
test('enabling maintenance is explicit, guards double submission and preserves an exact failed agreement',async()=>{
  const writes=[];let finish;
  await reactFixture(async({render,button,click,change,submit,update})=>{
    const {PanelMaintenance}=await setup();
    await render(PanelMaintenance,{panel,refresh:async()=>{}});
    assert.equal(writes.length,0);
    await click(button('Panel updates'));
    assert.equal(writes.length,0);
    const dialog=document.querySelector('dialog');
    await change(dialog.querySelector('textarea'),'Read assigned requests and update their waiting status');
    await change(dialog.querySelector('input[id$="-interval"]'),'15');
    await change(dialog.querySelector('input[id$="-age"]'),'30');
    await update(()=>{dialog.querySelector('form').dispatchEvent(new window.Event('submit',{bubbles:true,cancelable:true}));
      dialog.querySelector('form').dispatchEvent(new window.Event('submit',{bubbles:true,cancelable:true}));});
    assert.equal(writes.length,1);
    assert.equal(writes[0].body.interval_seconds,900);assert.equal(writes[0].body.stale_seconds,1800);
    assert.equal(writes[0].body.enabled,true);assert.equal(writes[0].body.panel_revision,1);
    await update(()=>finish(response({error:'Save acknowledgment unavailable'},503)));
    assert.match(dialog.textContent,/Save acknowledgment unavailable/);
    assert.equal(dialog.querySelector('textarea').value,'Read assigned requests and update their waiting status');
    await submit(dialog.querySelector('form'));assert.equal(writes.length,2);
    assert.deepEqual(writes[0].body,writes[1].body);
    await update(()=>finish(response({revision:1,enabled:true})));
    assert.equal(document.querySelector('dialog'),null);
  },{url,fetch:async(path,options={})=>{
    if(path.endsWith('/session'))return response(session);
    writes.push({path,body:JSON.parse(options.body)});return new Promise(resolve=>{finish=resolve;});
  }});
});
test('manual updates have exact retry identities, pending guards and no automatic retry after uncertain transport',async()=>{
  const writes=[];let finish;
  const current=structuredClone(panel);Object.assign(current.maintenance,{revision:4,enabled:true,status:'current',instructions:'Read assigned requests'});
  await reactFixture(async({render,button,update,click})=>{
    const {PanelMaintenance}=await setup();await render(PanelMaintenance,{panel:current,refresh:async()=>{}});
    await update(()=>{button('Update now').click();button('Update now').click();});assert.equal(writes.length,1);
    await update(()=>finish(response({error:'Connection lost'},503)));
    assert.equal(writes.length,1);assert.match(document.body.textContent,/Connection lost/);
    await click(button('Update now'));assert.deepEqual(writes[1],writes[0]);
    await update(()=>finish(response({state:'queued',reply:'task'})));
    await click(button('Update now'));assert.notEqual(writes[2].body.request,writes[0].body.request);
    await update(()=>finish(response({state:'queued',reply:'task'})));
    current.maintenance.status='updating';await render(PanelMaintenance,{panel:current,refresh:async()=>{}});
    assert.equal(button('Update now').disabled,true);
    current.maintenance.status='review_required';current.active_revision=2;
    await render(PanelMaintenance,{panel:current,refresh:async()=>{}});
    assert.equal(button('Update now').disabled,true);
    await click(button('Panel updates'));
    assert.equal(button('Save update agreement').disabled,false);
    await update(()=>document.querySelector('dialog form').dispatchEvent(new window.Event('submit',{bubbles:true,cancelable:true})));
    assert.equal(writes.at(-1).body.panel_revision,2);
    await update(()=>finish(response({revision:5,enabled:true})));
  },{url,fetch:async(path,options={})=>{
    if(path.endsWith('/session'))return response(session);
    writes.push({path,body:JSON.parse(options.body)});return new Promise(resolve=>{finish=resolve;});
  }});
});
test('pause uses the saved agreement even when an unsaved form draft differs and ineligible sources cannot enable updates',async()=>{
  const writes=[];const current=structuredClone(panel);
  Object.assign(current.maintenance,{revision:2,enabled:true,instructions:'Saved instructions',interval_seconds:900,status:'delayed'});
  await reactFixture(async({render,button,click,change})=>{
    const {PanelMaintenance}=await setup();await render(PanelMaintenance,{panel:current,refresh:async()=>{}});
    assert.match(document.body.textContent,/Update delayed/);
    await click(button('Panel updates'));await change(document.querySelector('textarea'),'Unsaved edit');
    await click(button('Pause updates'));
    assert.equal(writes.length,1);assert.equal(writes[0].body.enabled,false);
    assert.equal(writes[0].body.instructions,'Saved instructions');assert.equal(writes[0].body.revision,2);
    current.maintenance.enabled=false;current.maintenance.eligible=false;
    await render(PanelMaintenance,{panel:current,refresh:async()=>{}});await click(button('Panel updates'));
    assert.equal(button('Enable updates').disabled,true);
  },{url,fetch:async(path,options={})=>{
    if(path.endsWith('/session'))return response(session);
    writes.push({path,body:JSON.parse(options.body)});return response({revision:3,enabled:false});
  }});
});

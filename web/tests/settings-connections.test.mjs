import {test} from 'node:test';
import assert from 'node:assert/strict';
import {reactFixture} from './react-fixture.mjs';
test('Local connector setup preserves its API payload, failed drafts and explicit retry',async()=>{
 const writes=[];let failing=true;
 await reactFixture(async({render,button,click,change,submit})=>{
  const {LocalConnections}=await import('../src/react/settings/connections.tsx');
  await render(LocalConnections);assert.equal(writes.length,0);
  await click(button('Connect an account'));
  const form=document.querySelector('form');
  await change(form.querySelector('input'),'Work Gmail');await submit(form);
  assert.match(form.textContent,/Link not acknowledged/);
  assert.equal(form.querySelector('input').value,'Work Gmail');
  assert.deepEqual(writes[0],{toolkit:'gmail',auth_config_id:'config',label:'Work Gmail'});
  failing=false;await submit(form);
  assert.equal(writes.length,2);
  const link=document.querySelector('a');assert.equal(link.textContent,'Continue to service sign-in');
  assert.equal(link.rel,'noopener noreferrer');
 },{fetch:async(path,options={})=>{
  const body=options.body?JSON.parse(options.body):undefined;
  if(body){assert.equal(path,'/api/connections');writes.push(body);return {ok:!failing,json:async()=>failing?{error:'Link not acknowledged'}:{redirect_url:'https://example.test/sign-in'}};}
  const data=path.includes('/configs')?{configs:[{id:'config',name:'Gmail',toolkit:'gmail'}]}:{configured:true,connections:[]};
  return {ok:true,json:async()=>data};
 }});
});

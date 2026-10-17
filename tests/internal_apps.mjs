import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { mkdir } from 'node:fs/promises';
import { setup,until,root } from './fixtures/goose_app.mjs';
const { chromium } = createRequire(join(root,'web/package.json'))('playwright');

test('Goose writes a working internal app; previews isolate data and activated records need no model', {timeout:90000},async t=>{
  const app=await setup(t);
  const draft={id:randomUUID(),expected_revision:0,name:'Inventory fixture',description:'Synthetic local inventory app',html:'<h1>Inventory</h1><p id="boundary"></p><form><label>Item name<input id="name" required></label><label>Quantity<input id="quantity" type="number" value="1" required></label><button>Add item</button></form><p id="status" role="status"></p><ul id="items"></ul>',css:'body{font:16px sans-serif;padding:20px}label{display:block;margin:15px 0}input,button{padding:10px}',javascript:`
  const form=document.querySelector('form'),status=document.querySelector('#status');
  try{window.parent.document.querySelector('#app');document.querySelector('#boundary').textContent='Parent accessible';}catch{document.querySelector('#boundary').textContent='Parent blocked';}
  async function refresh(){const result=await ayati.records.list('items');const nodes=result.records.map(record=>{const li=document.createElement('li');li.textContent=record.values.name+' · '+record.values.quantity;return li});document.querySelector('#items').replaceChildren(...nodes);}
  form.onsubmit=async event=>{event.preventDefault();const button=form.querySelector('button');button.disabled=true;try{await ayati.records.save('items',{name:document.querySelector('#name').value,quantity:Number(document.querySelector('#quantity').value)});await refresh();status.textContent='Saved';}catch(error){status.textContent=error.message}finally{button.disabled=false}};
  refresh().catch(error=>status.textContent=error.message);
  `,collections:{items:{name:'string',quantity:'number'}}};
  async function approved(tool,args){
    const task=await app.submit('workspace '+JSON.stringify({tool,arguments:args}));
    const permission=await until(async()=>(await app.state()).permissions?.[0],'App approval missing');await app.request(`/permissions/${permission.id}`,{allow:true});assert.equal((await app.done(task.id)).task.status,'completed');
  }
  await approved('app_write',draft);
  const browser=await chromium.launch();t.after(()=>browser.close());const page=await browser.newPage({viewport:{width:390,height:844}});
  const policies=[];page.on('response',response=>{if(response.url().includes('/document/'))policies.push(response.headers()['content-security-policy']);});
  await page.goto(app.base.replace('/api','/?view=apps'));
  await page.getByRole('button',{name:'Preview latest release',exact:true}).click();
  let dialog=page.getByRole('dialog');let frame=dialog.frameLocator('iframe');
  await frame.getByText('Parent blocked',{exact:true}).waitFor();
  assert(policies[0].includes('sandbox allow-scripts'));
  await frame.getByLabel('Item name',{exact:true}).fill('Preview-only item');await frame.getByRole('button',{name:'Add item',exact:true}).click();await until(async()=>await frame.getByRole('status').textContent(),'No app save outcome');assert.equal(await frame.getByRole('status').textContent(),'Saved');await frame.getByText('Preview-only item · 1',{exact:true}).waitFor();
  await dialog.getByRole('button',{name:'Activate this release',exact:true}).click();
  await dialog.getByText('This exact release is active.',{exact:false}).waitFor();await dialog.getByRole('button',{name:'Close app',exact:true}).click();
  const calls=app.provider.requests.length;
  await page.getByRole('button',{name:'Open app',exact:true}).click();dialog=page.getByRole('dialog');frame=dialog.frameLocator('iframe');
  await frame.getByText('Parent blocked',{exact:true}).waitFor();assert.equal(await frame.getByText('Preview-only item · 1',{exact:true}).count(),0);
  await frame.getByLabel('Item name',{exact:true}).fill('Private live item');await frame.getByRole('button',{name:'Add item',exact:true}).click();await frame.getByText('Private live item · 1',{exact:true}).waitFor();
  assert.equal(app.provider.requests.length,calls,'Ordinary app data operations must not invoke the model');
  const denied=await page.frames().find(f=>f.url().includes('/document/')).evaluate(async()=>{try{await fetch('/api/health');return false}catch{return true}});assert.equal(denied,true);
  await mkdir(join(root,'.test-runs/apps'),{recursive:true});await page.screenshot({path:join(root,'.test-runs/apps/mobile.png'),fullPage:true});
  await dialog.getByRole('button',{name:'Close app',exact:true}).click();
  await page.getByRole('button',{name:'Manage agent access',exact:true}).click();await page.getByRole('button',{name:'Assign record access',exact:true}).click();
  const read=await app.submit('workspace '+JSON.stringify({tool:'app_records',arguments:{app_id:draft.id,command:{action:'list',collection:'items'}}}));
  assert.equal((await app.done(read.id)).task.status,'completed');assert((await app.state()).messages.some(m=>m.task_id===read.id&&m.content.includes('Private live item')));
  await page.getByRole('button',{name:'Remove record access',exact:true}).click();await until(async()=>(await app.request(`/apps/${draft.id}/grants`)).grants.length===0,'Record grant not removed');
  const rejected=await app.submit('workspace '+JSON.stringify({tool:'app_records',arguments:{app_id:draft.id,command:{action:'list',collection:'items'}}}));
  assert.notEqual((await app.done(rejected.id)).task.status,'completed');
  assert(!JSON.stringify(app.provider.requests.at(-1).messages).includes('Private live item'),'Revocation must discard stale working context');
  await app.stop();await app.start();await page.reload();await page.getByRole('button',{name:'Open app',exact:true}).click();
  await page.getByRole('dialog').frameLocator('iframe').getByText('Private live item · 1',{exact:true}).waitFor();
});

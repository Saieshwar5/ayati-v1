import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { join } from 'node:path';
import { mkdir } from 'node:fs/promises';
import { setup,until,root } from './fixtures/goose_app.mjs';
const { chromium } = createRequire(join(root,'web/package.json'))('playwright');

test('Goose creates a custom panel, owner keeps it, and scheduled updates use the saved policy', {timeout:60000},async t=>{
  const app=await setup(t);
  const panel={id:randomUUID(),expected_revision:0,title:'Sample stock outlook',purpose:'Synthetic agent findings; no live account data',sources:[],sections:[{kind:'metric',label:'At risk',value:'7'},{kind:'text',heading:'Assumption',body:'<img src=x onerror=alert(1)> is untrusted text'},{kind:'chart',title:'Sample trend',points:[{label:'Mon',value:2},{label:'Tue',value:7}]}]};
  const command=(tool,args)=>'workspace '+JSON.stringify({tool,arguments:args});
  async function approved(tool,args){
    const task=await app.submit(command(tool,args));
    const permission=await until(async()=>(await app.state()).permissions?.[0],'Workspace approval missing');
    await app.request(`/permissions/${permission.id}`,{allow:true});assert.equal((await app.done(task.id)).task.status,'completed');
  }
  await approved('panel_write',panel);
  const browser=await chromium.launch();t.after(()=>browser.close());
  const page=await browser.newPage({viewport:{width:390,height:844}});
  const calls=app.provider.requests.length;
  await page.goto(app.base.replace('/api','/?view=home'));
  await page.getByRole('heading',{name:panel.title,exact:true}).waitFor();
  assert.equal(await page.locator('.panel-snapshot img').count(),0);
  assert.equal(await page.locator('.panel-snapshot svg').count(),1);
  await page.getByLabel('Allow this agent to refresh this panel using these same sources',{exact:false}).check();
  await page.getByRole('button',{name:'Keep on Home',exact:true}).click();
  await page.getByRole('button',{name:'Pause panel updates',exact:true}).waitFor();
  assert.equal(app.provider.requests.length,calls,'Displaying/activating Home must not call the model');
  await mkdir(join(root,'.test-runs/panels'),{recursive:true});await page.screenshot({path:join(root,'.test-runs/panels/mobile.png'),fullPage:true});
  const updated={...panel,expected_revision:1,sections:[{kind:'metric',label:'At risk',value:'8'}]};
  await approved('schedule_create',{id:randomUUID(),panel_id:panel.id,content:command('panel_write',updated),run_at_ms:Date.now()+1800,interval_seconds:null});
  await until(async()=>(await app.request('/panels')).panels[0]?.active_revision===2,'Scheduled panel refresh did not activate');
  assert.equal((await app.state()).permissions.length,0,'The approved panel policy must avoid repeated update prompts');
  const active=(await app.request('/panels')).panels[0];assert.equal(active.active.definition.sections[0].value,'8');
  await app.stop();await app.start();
  assert.equal((await app.request('/panels')).panels[0].active_revision,2);
  await page.reload();await page.getByRole('button',{name:'Pause panel updates',exact:true}).waitFor();
  await page.getByRole('button',{name:'Pause panel updates',exact:true}).click();
  await page.getByRole('button',{name:'Allow panel updates',exact:true}).waitFor();
  assert.equal((await app.request('/panels')).panels[0].updates_enabled,false);
});

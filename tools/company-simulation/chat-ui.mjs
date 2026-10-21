// Retained UI rehearsal: real shared-chat APIs, deterministic agents, no LLM calls.
import { chromium } from '../../web/node_modules/playwright-core/index.mjs';
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { randomUUID } from 'node:crypto';
import assert from 'node:assert/strict';
const { directory } = JSON.parse(await readFile(resolve('.ayati-local/company-simulation/runs/latest.json')));
const c = JSON.parse(await readFile(join(directory,'manifest.json')));
const evidence = join(directory,`chat-ui-${Date.now()}`); await mkdir(evidence);
const browser = await chromium.launch({ headless:true, executablePath:'/usr/bin/chromium', args:['--no-sandbox','--use-fake-ui-for-media-stream','--use-fake-device-for-media-stream'] });
const checks=[], errors=[];
const check = async(name,fn)=>{ await fn(); checks.push(name); console.log(`PASS ${name}`); };
const headers={Origin:c.base,'X-Ayati-Request':'identity'};
try {
 const owner=await browser.newContext({viewport:{width:1280,height:900},permissions:['microphone']}), member=await browser.newContext();
 for(const [context,n] of [[owner,0],[member,3]]) assert.ok((await context.request.post(`${c.base}/api/identity/simulation/employees/${n}/login`,{headers,data:{}})).ok());
 const chat=randomUUID(), agent=c.agents[0];
 assert.ok((await owner.request.post(`${c.base}${c.w}/chats`,{headers,data:{id:chat,name:'Chat experience review',description:'Retained mention, markdown and media verification',kind:'channel',users:[c.employees[3].user.id],agents:[agent.id]}})).ok());
 const page=await owner.newPage(), other=await member.newPage();
 for(const p of [page,other]) p.on('pageerror',e=>errors.push(String(e)));
 const url=`${c.base}/w/${c.w.split('/').at(-1)}/?view=chat&conversation=${chat}`;
 await page.goto(url); await other.goto(url);
 const input=page.getByRole('textbox',{name:'Message',exact:true});await input.waitFor();
 await check('Markdown headings, lists, tables and code are formatted safely for both members',async()=>{
  await input.fill('# Weekly update\n\n**Ready for review**\n\n- Check stock\n- Prepare quote\n\n|Item|Count|\n|---|---|\n|Desk|12|\n\n```js\nconst ready = true;\n```\n\n<script>window.bad=true</script>');
  await page.getByRole('button',{name:'Send message',exact:true}).click();
  await other.getByRole('heading',{name:'Weekly update'}).waitFor();assert.equal(await other.locator('.message-markdown table').count(),1);assert.equal(await other.locator('.message-markdown pre code').count(),1);assert.equal(await other.evaluate(()=>window.bad),undefined);
 });
 await check('Keyboard agent mention produces a channel-visible reply without opening a drawer',async()=>{
  await input.fill('@Management');await page.getByRole('option').first().waitFor();await input.press('Enter');await input.press('End');await input.press('ControlOrMeta+End');await input.press('Space');await input.press('a');await input.press('Enter');
  await other.locator('.chat-message[aria-label^="Message from Management"] .message-markdown').filter({hasText:'Simulation'}).waitFor();
  assert.equal(await page.getByRole('dialog',{name:'Thread',exact:true}).count(),0);
  const h=await(await owner.request.get(`${c.base}${c.w}/chats/${chat}/messages`)).json();assert.ok(h.messages.find(m=>m.kind==='agent'&&m.state==='complete'&&m.thread===null));
 });
 await check('Human mentions retain identity and do not wake an agent',async()=>{
  await input.fill('@'+c.employees[3].name.slice(0,4));await page.getByRole('option').first().click();await input.press('End');await input.press('Enter');
  await other.locator('.message-mention').filter({hasText:c.employees[3].name}).waitFor();
  const h=await(await owner.request.get(`${c.base}${c.w}/chats/${chat}/messages`)).json();assert.equal(h.messages.filter(m=>m.kind==='agent').length,1);
 });
 await check('Files and reviewed voice recording use controls inside the shared composer',async()=>{
  assert.equal(await page.locator('.message-composer').getByRole('button',{name:'Attach file',exact:true}).count(),1);
  await page.getByLabel('Choose attachment').setInputFiles({name:'review.txt',mimeType:'text/plain',buffer:Buffer.from('Retained chat UI evidence')});await page.getByRole('button',{name:'Remove review.txt',exact:true}).waitFor();
  await page.getByRole('button',{name:'Record voice',exact:true}).click();await page.getByRole('button',{name:'Stop recording',exact:true}).waitFor();await page.waitForTimeout(1200);await page.getByRole('button',{name:'Stop recording',exact:true}).click();
  await page.getByLabel('Recording preview').waitFor();assert.ok(await page.getByRole('button',{name:'Send message',exact:true}).isDisabled());
  await page.getByRole('button',{name:'Attach recording',exact:true}).click();await page.getByRole('button',{name:'Remove voice-message.webm',exact:true}).waitFor();
  await page.getByRole('button',{name:'Send message',exact:true}).click();await other.getByRole('link',{name:'review.txt',exact:true}).waitFor();await other.getByLabel('voice-message.webm').waitFor();
 });
 await page.screenshot({path:join(evidence,'channel-desktop.png')});
 await check('Thread replies stay explicit and use the same composer with person suggestions',async()=>{
  await page.getByRole('button',{name:'Reply in thread',exact:true}).first().click();
  const dialog=page.getByRole('dialog',{name:'Thread',exact:true});const reply=dialog.getByRole('textbox',{name:'Reply in thread',exact:true});
  await reply.fill('@'+c.employees[3].name.slice(0,4));await dialog.getByRole('option').first().click();await dialog.getByRole('button',{name:'Send message',exact:true}).click();
  await dialog.locator('[aria-label="Thread replies"] .message-mention').waitFor();
  await dialog.getByRole('button',{name:'Close',exact:true}).click();
 });
 await check('Narrow screen keeps the composer and transcript inside the viewport',async()=>{
  await page.setViewportSize({width:360,height:740});
  await page.waitForFunction(()=>document.documentElement.scrollWidth<=innerWidth+1);
  assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1));
  await page.screenshot({path:join(evidence,'channel-narrow.png')});
 });
 await check('Private agent composer keeps drafts, formats responses and preserves tool controls',async()=>{
  // Only the browser's model endpoints are mocked; no provider or live agent call.
  await page.route('**/api/state',route=>route.fulfill({json:{messages:[{id:'fixture',task_id:'task',role:'assistant',content:'## Private answer\n\n**Formatted** response',state:'complete',created_at:''}],tasks:[],artifacts:[]}}));
  await page.route('**/api/health',route=>route.fulfill({json:{model_ready:true,file_uploads_supported:true}}));
  await page.goto(`${c.base}/w/${c.w.split('/').at(-1)}/?view=chat&agent=local-agent`);
  await page.getByRole('heading',{name:'Private answer'}).waitFor();
  const privateInput=page.locator('.message-composer textarea');await privateInput.fill('Private draft kept on reload');
  assert.equal(await page.locator('.message-composer').getByRole('button',{name:'Record voice',exact:true}).count(),1);
  await page.reload();await privateInput.waitFor();assert.equal(await privateInput.inputValue(),'Private draft kept on reload');
  await page.screenshot({path:join(evidence,'private-narrow.png')});
  const sent=[];let fail=true;
  await page.route('**/api/messages',async route=>{sent.push(route.request().postDataJSON());await route.fulfill({status:fail?503:200,json:fail?{error:'Temporary test outage'}:{ok:true}});});
  await page.getByRole('button',{name:'Send message',exact:true}).click();await page.getByRole('alert').filter({hasText:'Temporary test outage'}).waitFor();
  assert.equal(await privateInput.inputValue(),'Private draft kept on reload');fail=false;
  await page.getByRole('button',{name:'Send message',exact:true}).click();await page.waitForFunction(()=>document.querySelector('.message-composer textarea')?.value==='');
  assert.equal(sent.length,2);assert.equal(sent[0].request_id,sent[1].request_id);

 });
 assert.deepEqual(errors,[]);console.log(`Evidence: ${evidence}`);
} catch(e){errors.push(String(e));throw e;} finally {await writeFile(join(evidence,'results.json'),JSON.stringify({checks,errors},null,2));await browser.close();}

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { createServer, request as forwardRequest } from 'node:http';
import { once } from 'node:events';
import { setTimeout as delay } from 'node:timers/promises';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { until } from './fixtures/goose_app.mjs';

const options={skip:!process.env.RELAY_TEST_DATABASE_URL,timeout:120000};
const ready=app=>until(()=>Number(app.sql("SELECT count(*) FROM relay_executor_slots WHERE state='ready'"))>=2,'Prepared slots missing',35000);
const draft=async(app,job)=>until(async()=>{
  const message=(await app.history(job.chat)).find(m=>m.id===job.reply);
  return message?.progress?.text && message;
},'No durable partial output arrived',20000);

test('real Goose publishes a partial reply before completion and reconnect restores it',options,async t=>{
  const app=await workerFixture(t,{mode:'pooled',control:true});
  app.provider.setStream({chunks:['FIRST_VISIBLE 🦀',' and the final answer.'],holdAfter:1});
  app.provider.setOutputHeld(true);
  const executor=await app.startExecutor();await ready(app);
  const began=Date.now();const job=await app.send('Please stream this answer');
  const partial=await draft(app,job);
  const firstDraftObserved=Date.now();
  assert.equal(app.provider.requests[0].stream,true,'The pinned harness must request actual provider streaming');
  assert.equal(partial.state,'queued');assert.equal(partial.content,'');
  assert.equal(partial.progress.text,'FIRST_VISIBLE 🦀');assert.equal(partial.can_cancel,true);
  assert.equal(JSON.stringify(partial).includes('PRIVATE_REASONING_SENTINEL'),false);
  await app.stopApi();await app.startApi();
  const restored=(await app.history()).find(m=>m.id===job.reply);
  assert.equal(restored.progress.text,partial.progress.text);assert.equal(restored.revision,partial.revision);
  assert.equal(app.provider.requests.length,1);
  app.provider.setOutputHeld(false);const final=await app.done(job);
  assert.equal(final.content,'FIRST_VISIBLE 🦀 and the final answer.');assert.equal(final.progress,null);
  const timings=JSON.parse(app.sql(`SELECT row_to_json(a) FROM relay_worker_assignments a WHERE reply='${job.reply}'`));
  await writeFile(app.data+'/stream-timings.json',JSON.stringify({began_ms:began,first_draft_observed_ms:firstDraftObserved,assignment:timings,provider:app.provider.timings},null,2));
  await app.stop(executor);
  app.pass('Real pinned Goose streams visible text before completion; controller restart restores the draft without another inference or reasoning exposure');
});

test('Chat streams a real reply, preserves the composer and resumes after browser disconnection',options,async t=>{
 const app=await workerFixture(t,{mode:'pooled',control:true});
 app.provider.setStream({chunks:['BROWSER_PARTIAL 🦀',' grows',' into the final reply.'],holdAfter:1,delayMs:400});
 app.provider.setOutputHeld(true);const executor=await app.startExecutor();await ready(app);
 const {chromium}=createRequire(new URL('../web/package.json',import.meta.url))('@playwright/test');
 const browser=await chromium.launch();t.after(()=>browser.close());
 const context=await browser.newContext({viewport:{width:1200,height:800}});
 const [name,...value]=app.cookie.split('=');
 await context.addCookies([{name,value:value.join('='),url:app.base,httpOnly:true,sameSite:'Lax'}]);
 const page=await context.newPage();const errors=[];page.on('pageerror',e=>errors.push(e.message));
 const workspace=app.path.split('/').at(-1);
 await page.goto(`${app.base}/w/${workspace}/?view=chat&conversation=${app.conversation}`);
 const composer=page.getByRole('textbox',{name:'Message',exact:true});await composer.fill('Stream this reply through Chat');
 const began=Date.now();await page.getByRole('button',{name:'Send message',exact:true}).click();
 await page.getByText('BROWSER_PARTIAL 🦀',{exact:true}).waitFor();const firstVisible=Date.now();
 assert.equal(await page.getByRole('button',{name:'Stop reply'}).count(),1);
 await composer.fill('KEEP_UNSENT_DRAFT');await composer.focus();
 await page.screenshot({path:app.data+'/partial-desktop.png'});
 await context.setOffline(true);await delay(200);await context.setOffline(false);
 await page.reload();await page.getByText('BROWSER_PARTIAL 🦀',{exact:true}).waitFor();
 assert.equal(await composer.inputValue(),'KEEP_UNSENT_DRAFT');await composer.focus();
 app.provider.setOutputHeld(false);
 await page.getByText('BROWSER_PARTIAL 🦀 grows into the final reply.',{exact:true}).waitFor();
 assert.equal(await composer.inputValue(),'KEEP_UNSENT_DRAFT');assert.equal(await composer.evaluate(node=>node===document.activeElement),true);
 assert.equal(await page.getByRole('button',{name:'Stop reply'}).count(),0);assert.equal(app.provider.requests.length,1);
 assert.equal((await page.locator('[aria-label="Messages"]').textContent()).includes('PRIVATE_REASONING_SENTINEL'),false);
 await page.setViewportSize({width:390,height:760});await page.screenshot({path:app.data+'/final-mobile.png'});
 assert.deepEqual(errors,[]);
 await writeFile(app.data+'/browser-timing.json',JSON.stringify({receipt_click_ms:began,first_visible_ms:firstVisible,provider:app.provider.timings},null,2));
 app.provider.setStream({chunks:['CANCELLABLE_PARTIAL',' MUST_NOT_APPEAR'],holdAfter:1});app.provider.setOutputHeld(true);
 await composer.fill('Cancel this streaming reply');await page.getByRole('button',{name:'Send message',exact:true}).click();
 await page.getByText('CANCELLABLE_PARTIAL',{exact:true}).waitFor();
 const stoppedPids=JSON.parse(app.sql("SELECT COALESCE(json_agg(pid),'[]') FROM relay_executor_slots WHERE state='running'"));
 await page.getByRole('button',{name:'Stop reply'}).click();await page.getByText('Request cancelled.',{exact:true}).waitFor();
 await app.assertChildrenStopped(stoppedPids.map(Number),10000);
 app.provider.setOutputHeld(false);await delay(800);
 assert.equal((await page.locator('[aria-label="Messages"]').textContent()).includes('MUST_NOT_APPEAR'),false);
 await app.stop(executor);
 app.pass('Real browser send, live draft, offline/reload replay, composer draft/focus and mobile transcript pass without repeating inference');
});

test('lost output acknowledgments retry the exact batch without duplicating visible text',options,async t=>{
 const app=await workerFixture(t,{mode:'pooled',control:true});
 app.provider.setStream({chunks:['Exact 🦀',' text once.'],delayMs:250,holdAfter:9});
 const attempts=new Map();let dropped=false;
 const target=new URL(app.control.url);
 const proxy=createServer((request,response)=>{
  const output=request.url.endsWith('/output');const body=[];
  if(output)request.on('data',chunk=>body.push(chunk));
  const upstream=forwardRequest(new URL(request.url,target),{method:request.method,headers:{...request.headers,host:target.host}},incoming=>{
   if(!output){response.writeHead(incoming.statusCode,incoming.headers);incoming.pipe(response);return;}
   const chunks=[];incoming.on('data',chunk=>chunks.push(chunk));incoming.on('end',()=>{
    const update=JSON.parse(Buffer.concat(body));const key=update.sequence;
    const previous=attempts.get(key)??[];previous.push(JSON.stringify(update));attempts.set(key,previous);
    if(update.phase==='responding' && !dropped && incoming.statusCode===200){dropped=true;response.destroy();return;}
    response.writeHead(incoming.statusCode,incoming.headers);response.end(Buffer.concat(chunks));
   });
  });
  upstream.on('error',()=>response.destroy());response.on('close',()=>upstream.destroy());request.pipe(upstream);
 }).listen(0,'127.0.0.1');await once(proxy,'listening');
 t.after(()=>{proxy.closeAllConnections();proxy.close();});
 const executor=await app.startExecutor({AYATI_WORKER_CONTROL_URL:`http://127.0.0.1:${proxy.address().port}`});await ready(app);
 const job=await app.send('Do not duplicate output after a lost acknowledgment');
 const final=await app.done(job);assert.equal(final.content,'Exact 🦀 text once.');
 assert.equal(dropped,true);assert.ok([...attempts.values()].some(list=>list.length>1));
 for(const list of attempts.values())assert.equal(new Set(list).size,1,'A retry must retain its exact payload');
 assert.equal(app.provider.requests.length,1);assert.equal(app.attempts(job),1);
 const saved=JSON.parse(app.sql(`SELECT json_agg(text ORDER BY sequence) FROM relay_assignment_output WHERE assignment=(SELECT claim FROM relay_chat_jobs WHERE reply='${job.reply}')`));
 assert.equal(saved.join(''),'Exact 🦀 text once.');await app.stop(executor);
 app.pass('An actually dropped post-commit output acknowledgment retries identical data; one durable text and one model inference remain');
});

test('executor loss preserves an interrupted draft and replaces it with a clean attempt',options,async t=>{
 const app=await workerFixture(t,{mode:'pooled',control:true});
 app.provider.setStream({chunks:['OLD_ATTEMPT_DRAFT',' never finish'],holdAfter:1});app.provider.setOutputHeld(true);
 const executor=await app.startExecutor();await ready(app);const job=await app.send('Recover this interrupted reply');await draft(app,job);
 const pids=await app.gooseChildren(executor);await app.stop(executor,'SIGKILL');await app.assertChildrenStopped(pids);
 await until(async()=>(await app.history()).find(m=>m.id===job.reply)?.progress?.state==='interrupted','Expired partial output did not become interrupted',20000);
 app.provider.setStream({chunks:['NEW_ATTEMPT_DRAFT',' completed'],holdAfter:1});
 const replacement=await app.startExecutor();
 await until(async()=>(await app.history()).find(m=>m.id===job.reply)?.progress?.text==='NEW_ATTEMPT_DRAFT','Replacement did not produce clean output',25000);
 const restored=(await app.history()).find(m=>m.id===job.reply);
 assert.equal(restored.progress.generation,2);assert.equal(JSON.stringify(restored).includes('OLD_ATTEMPT_DRAFT'),false);
 assert.equal(app.sql("SELECT count(*) FROM relay_assignment_output WHERE text='OLD_ATTEMPT_DRAFT'"),'1');
 app.provider.setOutputHeld(false);assert.equal((await app.done(job)).content,'NEW_ATTEMPT_DRAFT completed');
 assert.equal(app.attempts(job),2);await app.stop(replacement);
 app.pass('An expired draft stays explicitly interrupted until a fenced replacement starts; old and new generations are never concatenated');
});

test('oversized visible output fails explicitly without publishing an oversized draft',options,async t=>{
 const app=await workerFixture(t,{mode:'pooled',control:true});
 app.provider.setStream({chunks:['x'.repeat(70000)],holdAfter:9});
 const executor=await app.startExecutor();await ready(app);const job=await app.send('Enforce the output bound');
 const failed=await app.done(job,'failed');assert.equal(failed.progress,null);assert.ok(failed.content.length<1000);
 assert.ok(Number(app.sql('SELECT COALESCE(sum(octet_length(text)),0) FROM relay_assignment_output'))<=65536);
 await ready(app);await app.stop(executor);
 app.pass('Output size/backpressure failure stops the harness and saves a terminal failure instead of losing or publishing unbounded text');
});

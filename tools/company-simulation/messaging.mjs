// Real browser evidence: durable outbox, receipts, ambiguity and live delivery.
import { chromium } from '../../web/node_modules/playwright-core/index.mjs';
import { readFile,writeFile,mkdir } from 'node:fs/promises';
import { resolve,join } from 'node:path';
import { randomUUID } from 'node:crypto';
import assert from 'node:assert/strict';
const {directory}=JSON.parse(await readFile(resolve('.ayati-local/company-simulation/runs/latest.json')));
const c=JSON.parse(await readFile(join(directory,'manifest.json')));
const evidence=join(directory,`messaging-${Date.now()}`);await mkdir(evidence);
const browser=await chromium.launch({headless:true,executablePath:'/usr/bin/chromium',args:['--no-sandbox']});
const checks=[],samples=[],errors=[];
const check=async(name,fn)=>{await fn();checks.push(name);console.log(`PASS ${name}`);};
const until=async(fn)=>{for(let i=0;i<150;i++){if(await fn())return;await new Promise(r=>setTimeout(r,100));}throw new Error('Condition timed out');};
try {
 const a=await browser.newContext(),b=await browser.newContext();
 const headers={Origin:c.base,'X-Ayati-Request':'identity'};
 for(const [context,number]of [[a,0],[b,3]])assert.ok((await context.request.post(`${c.base}/api/identity/simulation/employees/${number}/login`,{headers,data:{}})).ok());
 const chat=randomUUID();
 assert.ok((await a.request.post(`${c.base}${c.w}/chats`,{headers,data:{id:chat,name:`Messaging reliability ${new Date().toISOString()}`,kind:'channel',description:'Retained live-delivery and outage evidence',users:[c.employees[3].user.id],agents:[]}})).ok());
 const url=`${c.base}/w/${c.w.split('/').at(-1)}/?view=chat&conversation=${chat}`;
 const sender=await a.newPage(),receiver=await b.newPage();
 for(const p of [sender,receiver])p.on('pageerror',e=>errors.push(String(e)));
 await Promise.all([sender.goto(url),receiver.goto(url)]);
 for(const p of [sender,receiver])await p.getByText('Connected',{exact:true}).waitFor();
 const send=async(text)=>{await sender.getByRole('textbox',{name:'Message',exact:true}).fill(text);await sender.getByRole('button',{name:'Send message',exact:true}).click();};
 const history=async()=>{const r=await a.request.get(`${c.base}${c.w}/chats/${chat}/messages`);assert.ok(r.ok());return(await r.json()).messages;};
 await receiver.evaluate(()=>{Object.defineProperty(document,'hidden',{get:()=>true,configurable:true});Object.defineProperty(document,'hasFocus',{value:()=>false,configurable:true});document.dispatchEvent(new Event('visibilitychange'));});
 await check('Live delivery without periodic full-history reads',async()=>{
  let reads=0;receiver.on('request',r=>{if(r.method()==='GET'&&r.url().endsWith(`/chats/${chat}/messages`))reads++;});
  for(let i=0;i<12;i++){const text=`Live timing ${i} ${randomUUID()}`,start=performance.now();await send(text);await receiver.getByText(text,{exact:true}).waitFor();samples.push(performance.now()-start);}
  assert.equal(reads,0);
 });
 await check('Delivered is distinct from seen in a background conversation',async()=>{
  await until(async()=>{const h=await history();return h.length===12&&h.every(m=>m.delivered===1);});
  assert.ok((await history()).every(m=>m.seen===0));
 });
 await check('Visible foreground messages receive seen acknowledgements',async()=>{
  await receiver.evaluate(()=>{delete document.hidden;delete document.hasFocus;});await receiver.bringToFront();
  await receiver.evaluate(()=>window.dispatchEvent(new Event('focus')));
  await until(async()=>(await history()).some(m=>m.seen===1));
  assert.ok((await history()).some(m=>m.seen===0),'Offscreen history must not automatically become seen');
 });
 await check('Lost response reconciles exact ID after reload without duplicate storage',async()=>{
  let committed=false,blocked=true;const text=`Lost acknowledgement ${randomUUID()}`;
  const pattern=`**/chats/${chat}/messages`;
  await sender.route(pattern,async route=>{
   if(route.request().method()!=='POST')return route.continue();
   if(!blocked)return route.continue();
   if(!committed){const response=await route.fetch();assert.ok(response.ok());committed=true;}
   await route.abort('failed');
  });
  await send(text);await sender.getByText('◷ Checking delivery…',{exact:true}).waitFor();
  await sender.reload();await sender.getByText('◷ Checking delivery…',{exact:true}).waitFor();
  blocked=false;
  await until(async()=>(await sender.locator('[aria-label="Your pending message"]').count())===0);
  assert.equal((await history()).filter(m=>m.content===text).length,1);
  await sender.unroute(pattern);
 });
 await check('Offline sending retains history and later sends the saved message',async()=>{
  await a.setOffline(true);const count=await sender.locator('.chat-message').count();
  const text=`Offline outbox ${randomUUID()}`;await send(text);
  await sender.getByText('◷ Checking delivery…',{exact:true}).waitFor();
  assert.ok(await sender.locator('.chat-message').count()>=count);
  await a.setOffline(false);
  await receiver.getByText(text,{exact:true}).waitFor({timeout:20000});
  await until(async()=>(await sender.locator('[aria-label="Your pending message"]').count())===0);
  assert.equal((await history()).filter(m=>m.content===text).length,1);
 });
 await check('Thread replies remain unread until the actual thread is viewed',async()=>{
  const root=(await history())[0];const id=randomUUID();
  assert.ok((await a.request.post(`${c.base}${c.w}/chats/${chat}/messages`,{headers,data:{id,content:'Unopened thread evidence',thread:root.id,attachments:[]}})).ok());
  await new Promise(r=>setTimeout(r,2000));assert.equal((await history()).find(m=>m.id===id).seen,0);
 });
 await check('Disconnected recipients replay missed messages on reconnect',async()=>{
  await b.setOffline(true);
  await until(async()=>(await receiver.locator('.chat-connection').textContent())!=='Connected');
  const missed=[0,1,2].map(i=>`Catch-up ${i} ${randomUUID()}`);
  for(const text of missed)await send(text);
  await until(async()=>(await history()).filter(m=>missed.includes(m.content)).length===3);
  await b.setOffline(false);
  for(const text of missed)await receiver.getByText(text,{exact:true}).waitFor({timeout:20000});
 });
 await check('Revocation stops live content and removes the protected view',async()=>{
  assert.ok((await a.request.post(`${c.base}${c.w}/chats/${chat}/members`,{headers,data:{users:[c.employees[0].user.id]}})).ok());
  await until(async()=>!(await receiver.locator('.chat-message').count()));
  await send('After revocation - owner only');await new Promise(r=>setTimeout(r,1000));
  assert.equal(await receiver.getByText('After revocation - owner only',{exact:true}).count(),0);
 });
 await sender.screenshot({path:join(evidence,'message-states.png')});
 await sender.setViewportSize({width:320,height:700});assert.ok(await sender.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
 await sender.screenshot({path:join(evidence,'narrow.png')});
 assert.deepEqual(errors,[]);
 samples.sort((x,y)=>x-y);
 const report={checks,samples_ms:samples,p95_ms:samples[Math.ceil(samples.length*.95)-1],errors,chat};
 await writeFile(join(evidence,'results.json'),JSON.stringify(report,null,2));
 console.log(JSON.stringify({evidence,...report},null,2));
} finally {await browser.close();}

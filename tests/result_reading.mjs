import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import { readFile, mkdir, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { randomUUID, createHash } from 'node:crypto';
import { workerFixture } from './fixtures/independent_worker.mjs';
import { until, root } from './fixtures/polling.mjs';
const { chromium } = createRequire(join(root, 'web/package.json'))('@playwright/test');
const evidence = join(root, '.tmp/result-reading');
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
async function openResponse(message) {
  await message.getByRole('button',{name:'Expand response',exact:true}).click();
  await message.getByRole('button',{name:'Open response reader',exact:true}).click();
}

test('long real Goose replies remain compact, freeze while reading and preserve exact saved downloads and passive files', {
  skip: !process.env.RELAY_TEST_DATABASE_URL, timeout: 180000,
}, async t => {
  let browser, app, page; const errors = [];
  t.after(async () => {
    if (page && app && !t.passed) { await page.screenshot({path:join(app.data,'reader-failure.png')}).catch(()=>{}); await writeFile(join(app.data,'reader-failure.html'),await page.content()).catch(()=>{}); }
    await browser?.close();
  });
  app = await workerFixture(t,{mode:'pooled',control:true,capacity:1,workspaceLimit:1,modelLimit:1,leaseSeconds:15});
  const first = '# Weekly operations report\n\n' + Array.from({length:55},(_,i)=>`## Section ${i+1}\n\nA saved business update with detail, context and next actions.\n\n`).join('');
  const tail = '\n## Final recommendation\n\nFINAL_SAVED_RESULT 🦀\n';
  app.provider.setStream({chunks:[first,tail],holdAfter:1}); app.provider.setOutputHeld(true);
  const executor = await app.startExecutor({AYATI_EXECUTOR_SLOTS:'1'});
  const job = await app.send('Prepare the long saved weekly report');
  await until(async()=>(await app.history()).find(m=>m.id===job.reply)?.progress?.text===first,'Long draft not saved',35000);
  const endpoint = `${app.path}/chats/${job.chat}/messages/${job.reply}/response`;
  const draft = (await app.request(endpoint,app.cookie)).value;
  assert.equal(draft.body,first); assert.equal(draft.version,hash(first)); assert.equal(draft.state,'draft');
  const compact = (await app.request(`${app.path}/chats/${job.chat}/messages?compact=true`,app.cookie)).value.messages.find(m=>m.id===job.reply);
  assert.ok([...compact.progress.text].length<=320); assert.equal(compact.response.version,draft.version);
  browser = await chromium.launch({headless:true});
  const context = await browser.newContext({viewport:{width:1440,height:960},permissions:['clipboard-read','clipboard-write']});
  const [name,value]=app.cookie.split('='); await context.addCookies([{name,value,url:app.base,httpOnly:true,sameSite:'Lax'}]);
  page = await context.newPage(); page.on('pageerror',error=>errors.push(error.message));
  const w=app.path.split('/').at(-1), url=`${app.base}/w/${w}/?view=chat&conversation=${job.chat}`;
  await page.goto(url);
  const message=page.locator(`[data-message-id="${job.reply}"]`);
  await message.getByRole('button',{name:'Expand response',exact:true}).waitFor();
  assert.equal(await page.getByRole('button',{name:'Check progress',exact:true}).count(),0);
  assert.ok((await message.locator('.response-excerpt').boundingBox()).height<=157);
  assert.ok((await message.textContent()).length<900);
  const composer=page.getByRole('textbox',{name:'Message',exact:true}); await composer.fill('KEEP_READER_DRAFT');
  const calls=app.provider.requests.length;
  await message.getByRole('button',{name:'Expand response',exact:true}).click();
  await message.getByText('Section 55',{exact:true}).waitFor();
  await page.screenshot({path:join(app.data,'corner-expanded-desktop.png')});
  await message.getByRole('button',{name:'Open response reader',exact:true}).click();
  const reader=page.getByRole('dialog',{name:'Agent response',exact:true});
  await reader.getByText('Section 55',{exact:true}).waitFor();
  await reader.locator('.result-body').evaluate(node=>{node.scrollTop=280;});
  const position=await reader.locator('.result-body').evaluate(node=>node.scrollTop);
  const readerTop=(await reader.locator('.result-body').boundingBox()).y;
  await page.screenshot({path:join(app.data,'reader-draft-desktop.png')});
  app.provider.setOutputHeld(false); const complete=await app.done(job); assert.equal(complete.content,first+tail);
  await until(()=>reader.getByRole('button',{name:'Load latest saved response',exact:true}).isEnabled(),'New saved state not offered',12000);
  assert.equal((await reader.locator('.result-body').boundingBox()).y,readerTop);
  assert.equal(await reader.getByText('FINAL_SAVED_RESULT 🦀',{exact:true}).count(),0);
  assert.equal(await reader.locator('.result-body').evaluate(node=>node.scrollTop),position);
  await reader.getByRole('button',{name:'Load latest saved response',exact:true}).click();
  await reader.getByText('FINAL_SAVED_RESULT 🦀',{exact:true}).waitFor();
  await reader.getByRole('button',{name:'Copy',exact:true}).click(); assert.equal(await page.evaluate(()=>navigator.clipboard.readText()),first+tail);
  const initialWidth=(await reader.boundingBox()).width; await reader.getByRole('button',{name:'Expand',exact:true}).click(); assert.ok((await reader.boundingBox()).width>initialWidth); await reader.getByRole('button',{name:'Reduce',exact:true}).click();
  const downloadEvent=page.waitForEvent('download'); await reader.getByRole('link',{name:'Download Markdown',exact:true}).click();
  const download=await downloadEvent; assert.equal(await readFile(await download.path(),'utf8'),first+tail);
  const old=await fetch(app.base+endpoint+`?download=true&version=${draft.version}`,{headers:{Cookie:app.cookie}}); assert.equal(old.status,409);
  await page.keyboard.press('Escape'); await reader.waitFor({state:'detached'});
  assert.equal(await composer.inputValue(),'KEEP_READER_DRAFT');
  assert.equal(await message.getByRole('button',{name:'Open response reader',exact:true}).evaluate(node=>node===document.activeElement),true);
  assert.equal(app.provider.requests.length,calls);
  await app.stop(executor); await app.stopApi(); await app.startApi(); await page.reload();
  assert.equal((await app.request(endpoint,app.cookie)).value.body,first+tail);
  assert.equal(await composer.inputValue(),'KEEP_READER_DRAFT');
  await openResponse(message); await reader.getByText('FINAL_SAVED_RESULT 🦀',{exact:true}).waitFor();
  await page.setViewportSize({width:390,height:844}); await page.emulateMedia({colorScheme:'dark'});
  await page.screenshot({path:join(app.data,'reader-complete-mobile-dark.png')});
  assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
  await page.keyboard.press('Escape');
  await message.getByRole('button',{name:'Collapse response',exact:true}).click();
  for (const [width,height,dark] of [[1440,960,false],[390,844,true],[320,568,false]]) {
    await page.setViewportSize({width,height}); await page.emulateMedia({colorScheme:dark?'dark':'light'});
    await message.scrollIntoViewIfNeeded();
    assert.ok((await message.locator('.chat-response').boundingBox()).height<=226);
    const arrow=message.getByRole('button',{name:'Expand response',exact:true});
    const target=await arrow.boundingBox();assert.ok(target.width>=44 && target.height>=44);
    assert.equal(await arrow.textContent(),'');
    assert.equal(await page.evaluate(()=>document.documentElement.scrollWidth>innerWidth),false);
    await page.screenshot({path:join(app.data,`corner-compact-${width}.png`)});
    await arrow.focus();await page.keyboard.press('Enter');
    await message.getByText('FINAL_SAVED_RESULT 🦀',{exact:true}).waitFor();
    assert.ok((await message.locator('.chat-response').boundingBox()).height>226);
    assert.equal(await message.locator('.inline-response-body').evaluate(node=>getComputedStyle(node).overflowY),'visible');
    await message.getByRole('button',{name:'Collapse response',exact:true}).click();
    assert.equal(await composer.inputValue(),'KEEP_READER_DRAFT');
  }
  await page.setViewportSize({width:1440,height:960}); await page.emulateMedia({colorScheme:'light'});
  app.pass('A real pinned Goose long stream stays compact, preserves a frozen reading position, updates explicitly and downloads the exact final body across controller/browser restart without another inference');

  async function upload(name,bytes,mime) {
    const response=await fetch(`${app.base}${app.path}/chats/${job.chat}/objects?name=${encodeURIComponent(name)}`,{method:'POST',headers:{Cookie:app.cookie,Origin:app.base,'X-Ayati-Request':'identity','Content-Type':mime},body:bytes});
    assert.equal(response.status,200,await response.clone().text()); return response.json();
  }
  const image=await readFile(join(root,'designs/result-reading/reader-concept-mobile.png'));
  const object=await upload('Business reference.png',image,'image/png');
  await app.request(`${app.path}/chats/${job.chat}/messages`,app.cookie,{id:randomUUID(),content:'Saved reference file',attachments:[object.id]});
  const imagePath=`${app.path}/objects/${object.id}`;
  const thumb=await fetch(app.base+imagePath+'?thumbnail=true',{headers:{Cookie:app.cookie}}); assert.equal(thumb.status,200);
  const thumbnail=Buffer.from(await thumb.arrayBuffer()); assert.ok(thumbnail.readUInt32BE(16)<=256); assert.ok(thumbnail.readUInt32BE(20)<=256); assert.ok(thumbnail.length<image.length);
  const originals=[]; page.on('request',request=>{if(request.url().includes(`/objects/${object.id}`)&&!request.url().includes('thumbnail=true'))originals.push(request.url());});
  await page.reload(); await page.getByRole('button',{name:'Open Business reference.png',exact:true}).waitFor(); assert.deepEqual(originals,[]);
  await page.getByRole('button',{name:'Open Business reference.png',exact:true}).click();
  const viewer=page.getByRole('dialog',{name:'Business reference.png',exact:true}); await viewer.locator('img').waitFor();
  await until(()=>viewer.locator('img').evaluate(node=>node.complete&&node.naturalWidth===390),'Image did not load');
  await viewer.getByRole('button',{name:'Zoom in',exact:true}).click();
  assert.ok(Math.abs((await viewer.locator('img').boundingBox()).width-390*1.25)<1);
  assert.equal(await viewer.locator('output').textContent(),'125%'); await viewer.getByRole('button',{name:'Fit',exact:true}).click();
  await page.screenshot({path:join(app.data,'image-viewer-desktop.png')});
  const originalEvent=page.waitForEvent('download'); await viewer.getByRole('link',{name:'Download original',exact:true}).click(); const original=await originalEvent;
  assert.equal(hash(await readFile(await original.path())),hash(image)); assert.equal(original.suggestedFilename(),'Business reference.png');
  await page.keyboard.press('Escape');
  const active=await upload('unsafe.svg',Buffer.from('<svg onload="alert(1)"><text>SAFE_TEXT_PREVIEW</text></svg>'),'image/svg+xml');
  await app.request(`${app.path}/chats/${job.chat}/messages`,app.cookie,{id:randomUUID(),content:'Inspect as text only',attachments:[active.id]});
  await page.getByRole('button',{name:'unsafe.svg',exact:true}).click(); const text=page.getByRole('dialog',{name:'unsafe.svg',exact:true});
  await text.getByText('<svg onload="alert(1)"><text>SAFE_TEXT_PREVIEW</text></svg>',{exact:true}).waitFor(); assert.equal(await text.locator('svg,img').count(),0);
  await page.keyboard.press('Escape');
  const stranger=await app.login(1); await app.request(imagePath,stranger,undefined,400);
  await app.request(`${imagePath}?thumbnail=true`,stranger,undefined,400);
  await app.request(`${imagePath}?version=wrong`,app.cookie,undefined,400);
  assert.equal(app.provider.requests.length,calls); assert.deepEqual(errors,[]);
  app.pass('Actual PNG thumbnails are bounded; full bytes load only after opening; original download preserves bytes and filename; active SVG is plain text and wrong-scope/version requests are denied');
  app.provider.setStream({chunks:[first+tail],holdAfter:9});
  const threadRoot=randomUUID(); await app.request(`${app.path}/chats/${job.chat}/messages`,app.cookie,{id:threadRoot,content:'Prepare the report in this thread'});
  const threadWorker=await app.startExecutor({AYATI_EXECUTOR_SLOTS:'1'});
  await app.request(`${app.path}/chats/${job.chat}/messages`,app.cookie,{id:randomUUID(),content:'Save a report in this thread',agent:app.profile.id,thread:threadRoot});
  const threadJob=JSON.parse(app.sql(`SELECT j.payload FROM relay_chat_jobs j JOIN relay_chat_messages m ON m.workspace=j.workspace AND m.id=j.reply WHERE m.thread='${threadRoot}' ORDER BY j.sequence DESC LIMIT 1`)); await app.done(threadJob);
  await page.goto(`${url}&message=${threadJob.reply}`);
  const thread=page.getByRole('dialog',{name:'Thread',exact:true}); await thread.waitFor();
  await openResponse(thread.locator(`[data-message-id="${threadJob.reply}"]`));
  await reader.getByText('FINAL_SAVED_RESULT 🦀',{exact:true}).waitFor(); await page.keyboard.press('Escape'); await reader.waitFor({state:'detached'}); assert.equal(await thread.isVisible(),true);
  await page.keyboard.press('Escape');await thread.waitFor({state:'detached'});await app.stop(threadWorker);
  app.pass('A real threaded reply opens its saved reader through a deep link; Escape closes only the reader and returns to the original thread');

  await writeFile(join(evidence,'managed-result-evidence.json'),JSON.stringify({data:app.data,body_version:hash(first+tail),image_version:hash(image),model_requests:app.provider.requests.length,errors},null,2));
});


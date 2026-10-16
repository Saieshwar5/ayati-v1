import { test } from 'node:test';
import assert from 'node:assert/strict';
import { randomUUID } from 'node:crypto';
import { createRequire } from 'node:module';
import { readFile, mkdir } from 'node:fs/promises';
import { join } from 'node:path';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { setup, until, vmManifest, root } from './fixtures/goose_app.mjs';
import { browserSite } from './fixtures/browser_site.mjs';

assert.ok(vmManifest, 'Set AYATI_VM_TEST_MANIFEST to a disposable browser-enabled guest');
const origin = 'http://fixture.ayati.invalid';

async function guestBrowserStatus(app) {
  const manifest = JSON.parse(await readFile(vmManifest,'utf8'));
  const status = await app.request('/vm/status');
  return new Promise((resolve,reject) => {
    const child = execFile('ssh',['-T','-F','/dev/null','-o','BatchMode=yes','-o','StrictHostKeyChecking=yes','-o','GlobalKnownHostsFile=/dev/null','-o',`UserKnownHostsFile=${join(manifest.directory,'known_hosts')}`,'-o','HostKeyAlias=ayati-local-agent','-i',join(manifest.directory,'client_key'),'-p',String(status.ssh_port),'ayati@127.0.0.1'],{timeout:10000},(error,stdout)=>error?reject(error):resolve(JSON.parse(stdout)));
    child.stdin.end(JSON.stringify({version:1,agent_id:'local-agent',op:'browser',browser:{op:'status'}})+'\n');
  });
}

async function context(t, idle = '0') {
  let app;
  // Failed tests must release their own synthetic controller before fixture
  // shutdown; a later test must not inherit an abandoned owner capability.
  t.after(async () => {
    if (!app) return;
    try {
      if ((await app.request('/browser/status')).owner) await promisify(execFile)(process.env.AYATI_TEST_BIN ?? join(root,'target/debug/ayati'),['browser','release'],{
        cwd:app.data,env:{PATH:process.env.PATH,AYATI_RUNTIME:'goose',AYATI_EXECUTION:'vm',AYATI_PORT:new URL(app.base).port,AYATI_DATA_DIR:app.data},
      });
    } catch { /* Preserve original failure and private fixture evidence. */ }
  });
  const site = await browserSite(); t.after(site.close);
  app = await setup(t, false, { AYATI_BROWSER: '1', AYATI_BROWSER_FIXTURE_PORT: String(site.port), AYATI_VM_IDLE_SECONDS: idle });
  let token = randomUUID();
  return { ...app, site, token,
    control: (op, extra = {}) => app.request('/browser/control', { op, token, ...extra }),
    async tool(tool, args = {}, allow = true, allowToolFailure = false) {
      const run = await app.submit('browser ' + JSON.stringify(Array.isArray(tool) ? {steps:tool} : { tool, arguments: args }));
      const decided = new Set();
      await until(async () => {
        const state = await app.state();
        const task = state.tasks.find((r) => r.id === run.id);
        if (['failed','completed','interrupted'].includes(task?.status)) return true;
        for (const permission of state.permissions.filter((p) => p.task_id === run.id && !decided.has(p.id))) {
          decided.add(permission.id); await app.request(`/permissions/${permission.id}`, { allow });
        }
        return false;
      }, 'Browser run did not finish',90000);
      assert.ok(decided.size > 0,'No browser approval was presented');
      const result = await app.done(run.id);
      assert.equal(result.task.status, 'completed', result.task.detail);
      const { actions } = await app.request(`/tasks/${run.id}/actions`);
      if (allow && !allowToolFailure) for (const action of actions) assert.equal(action.status,'completed',JSON.stringify(action.result));
      return result.state.messages.filter((m) => m.task_id === run.id && m.role === 'assistant').map((m) => m.content).join('');
    },
  };
}

test('real guest Goose sees only allowed MCP browser tools and navigates a synthetic website', { timeout: 120000 }, async (t) => {
  const app = await context(t);
  const opened = await app.tool('open', { url: origin });
  assert.match(opened, /Ayati browser fixture/);
  const tools = app.provider.requests.at(-1).tools.map((t) => t.function.name);
  assert.ok(tools.includes('ayati_browser__agent_browser_snapshot'));
  for (const forbidden of ['eval','cookies_get','get_cdp_url','profiles','set_headers']) assert.ok(!tools.includes(`ayati_browser__agent_browser_${forbidden}`));
  const snapshot = await app.tool('snapshot');
  assert.match(snapshot, /Reference/); assert.match(snapshot, /Save once/);
  assert.ok(app.site.requests.some((r) => r.path === '/'));
  const rejected = await app.tool('open', { url: origin + '/popup' }, false);
  assert.ok(!app.site.requests.some((r) => r.path === '/popup'));
  assert.doesNotMatch(rejected, /Popup page/);
});

test('real Chrome reaches public HTTPS while guest-to-host browser traffic stays blocked', { timeout: 120000 }, async (t) => {
  const app = await context(t);
  assert.match(await app.tool('open',{url:'https://example.com'}),/Example Domain/);
  const frame = await app.control('frame');
  assert.ok(frame.content.some((item)=>item.type==='image' && item.mimeType==='image/jpeg'));
  await app.tool('open',{url:`http://10.0.2.2:${app.site.port}/private-sentinel`},true,true);
  assert.ok(!app.site.requests.some((request)=>request.path==='/private-sentinel'));
  await app.tool('open',{url:origin});
  await app.tool([{tool:'snapshot'},{tool:'tab_new',arguments:{url:origin+'/popup'}}]);
  assert.match(await app.tool('tab_list'),/popup/);
  await app.tool([{tool:'snapshot'},{tool:'tab_close'}]);
});

test('Stop during browser takeover boot wins over the old wake request', { timeout: 120000 }, async (t) => {
  const app = await context(t);
  await app.request('/vm/stop',{});
  const taking = fetch(app.base+'/browser/control',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({op:'take',token:app.token})});
  await until(async ()=>(await app.request('/vm/status')).state==='changing','No takeover boot');
  const stopping = app.request('/vm/stop',{});
  assert.equal((await taking).status,400);
  await stopping;
  assert.equal((await app.request('/vm/status')).state,'off');
  assert.equal(app.provider.requests.length,0);
  const queued = await app.request('/intake/events',{source:'takeover-race',event_id:randomUUID(),content:'Parked until explicit owner Start'});
  await app.control('take'); await app.control('resume');
  assert.equal((await app.state()).tasks.find((r)=>r.id===queued.id).status,'queued');
  await app.request('/vm/start',{});
  assert.equal((await app.done(queued.id)).task.status,'completed');
});

test('owner controls the same real browser; disconnect/restart cannot resume work or leak typed secrets', { timeout: 180000 }, async (t) => {
  const app = await context(t, '1');
  await app.tool('open', { url: origin + '/login' });
  await app.control('take');
  const frame = await app.control('frame');
  assert.ok(frame.content.some((c) => c.type === 'image'));
  await app.control('input', { tool: 'agent_browser_press', arguments: { key: 'Tab' } });
  await app.control('input', { tool: 'agent_browser_press', arguments: { key: 'Tab' } });
  const secret = 'OWNER_ONLY_FIXTURE_SECRET';
  await app.control('input', { tool: 'agent_browser_keyboard_insert_text', arguments: { text: secret } });
  const queued = await app.submit('Queued during takeover');
  await new Promise((resolve) => setTimeout(resolve, 2200));
  assert.equal((await app.request('/vm/status')).state, 'ready');
  assert.equal((await app.state()).tasks.find((r) => r.id === queued.id).status, 'queued');
  const foreign = await fetch(app.base + '/browser/control', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ op:'input',token:randomUUID(),tool:'agent_browser_press',arguments:{key:'Enter'} }) });
  assert.equal(foreign.status,400);
  assert.ok(!JSON.stringify(app.provider.requests).includes(secret));
  assert.ok(!JSON.stringify(await app.state()).includes(secret));
  await app.stop(); await app.start();
  assert.equal((await app.request('/browser/status')).owner,true);
  assert.equal((await app.request('/vm/status')).state,'off');
  await app.control('take');
  await app.control('resume');
  assert.equal((await app.done(queued.id)).task.status,'completed');
});

test('web owner panel maps input to the guest and preserves browser profile through shutdown', { timeout: 180000 }, async (t) => {
  const app = await context(t);
  await app.tool('open', { url: origin });
  const { chromium } = createRequire(new URL('../web/package.json', import.meta.url))('@playwright/test');
  const browser = await chromium.launch(); t.after(() => browser.close());
  const page = await browser.newPage({ viewport: { width: 1200, height: 900 } });
  await page.goto(app.base.replace('/api',''));
  await page.getByRole('button',{name:'Browser',exact:true}).click();
  await page.getByRole('button',{name:'Take control',exact:true}).click();
  await page.getByText('You control the browser.',{exact:false}).waitFor();
  await until(() => page.locator('#browser-canvas').evaluate((canvas) => canvas.getContext('2d').getImageData(60,60,1,1).data[3] > 0),'No guest frame');
  // Focus a real guest input using mapped pointer coordinates (fixture layout).
  const canvas = page.locator('#browser-canvas');
  const box = await canvas.boundingBox();
  const width = await canvas.evaluate((c) => c.width);
  await page.mouse.click(box.x + 100 * box.width / width, box.y + 150 * box.width / width);
  await page.getByLabel('Type into the selected browser field').fill('owner-web-value');
  await page.getByRole('button',{name:'Send text',exact:true}).click();
  await canvas.focus(); await page.keyboard.press('Tab'); await page.keyboard.press('Enter');
  await until(() => app.site.writes.length === 1,'Owner did not submit guest form');
  assert.match(app.site.writes[0],/owner-web-value/);
  await page.getByRole('button',{name:'Hand back to agent',exact:true}).click();
  await until(async () => !(await app.request('/browser/status')).owner,'No handback');
  await app.tool('open',{url:origin});
  assert.ok(app.site.requests.some((r) => r.path === '/' && r.cookie.includes('ayati_fixture=logged-in')), 'Login cookie was not accepted before shutdown');
  const beforeShutdown = app.site.requests.length;
  console.log('Browser profile before shutdown:',JSON.stringify(await guestBrowserStatus(app)));
  await app.request('/vm/stop',{}); await app.request('/vm/start',{});
  console.log('Browser profile after shutdown:',JSON.stringify(await guestBrowserStatus(app)));
  await app.tool('open',{url:origin});
  assert.ok(app.site.requests.slice(beforeShutdown).some((r) => r.path === '/' && r.cookie.includes('ayati_fixture=logged-in')), 'Login cookie did not survive clean VM shutdown');
  const manifest = JSON.parse(await readFile(vmManifest,'utf8'));
  assert.equal(manifest.browser_version,'0.39.1');
  console.log('Browser VM resources:', JSON.stringify(await app.request('/vm/status')));
  await until(() => canvas.evaluate((c) => {
    const bytes = c.getContext('2d').getImageData(0,0,Math.min(c.width,500),Math.min(c.height,220)).data;
    let ink = 0;
    for (let i=0;i<bytes.length;i+=4) if (bytes[i+3] && bytes[i]<100 && bytes[i+1]<100 && bytes[i+2]<100) ink++;
    return ink>100;
  }),'Guest page was not visible in the owner panel');
  await mkdir(join(root,'.test-runs/browser'),{recursive:true});
  await page.screenshot({path:join(root,'.test-runs/browser/owner-panel.png')});
  await page.getByRole('button',{name:'Take control',exact:true}).click();
  await page.getByText('You control the browser.',{exact:false}).waitFor();
  await page.getByRole('button',{name:'Close browser session',exact:true}).click();
  await until(()=>page.locator('#browser-panel').isHidden(),'Browser session did not close');
  assert.equal((await app.request('/browser/status')).guest.closed,true);
});

test('read-only browsing sleeps automatically while unfinished forms require an explicit close', { timeout: 180000 }, async (t) => {
  const app = await context(t,'1');
  await app.tool('open',{url:origin});
  await until(async ()=>(await app.request('/vm/status')).state==='off','Read-only browser did not release its VM');
  await app.tool('open',{url:origin});
  await app.tool([{tool:'snapshot'},{tool:'fill',refName:'Reference',arguments:{text:'Unfinished form'}}]);
  const calls = app.provider.requests.length;
  await new Promise((resolve)=>setTimeout(resolve,2200));
  assert.equal((await app.request('/vm/status')).state,'ready');
  assert.equal((await app.request('/browser/status')).guest.interaction_pending,true);
  assert.equal(app.provider.requests.length,calls);
  await app.control('take'); await app.control('finish');
  const lateFrame = await fetch(app.base+'/browser/control',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({op:'frame',token:app.token})});
  assert.equal(lateFrame.status,400);
  await until(async ()=>(await app.request('/vm/status')).state==='off','Explicit close did not release the browser VM');
  await app.tool('open',{url:origin});
  await app.tool([{tool:'snapshot'},{tool:'fill',refName:'Reference',arguments:{text:'Owner approved close'}}]);
  await app.tool('close');
  await until(async ()=>(await app.request('/vm/status')).state==='off','Approved MCP close did not release the VM');
});

test('agent forms and bounded uploads/downloads use only private transfer files', { timeout: 180000 }, async (t) => {
  const app = await context(t);
  await app.tool('open',{url:origin});
  await app.tool([
    {tool:'snapshot'}, {tool:'fill',refName:'Reference',arguments:{text:'agent-form-value'}},
    {tool:'snapshot'}, {tool:'click',refName:'Save once'},
  ]);
  await until(() => app.site.writes.length === 1,'Agent did not submit the form');
  assert.match(app.site.writes[0],/agent-form-value/);
  await app.control('take');
  await app.control('put_file',{file:'sample.txt',data:Buffer.from('Synthetic owner upload').toString('base64')});
  await app.control('resume');
  await app.tool('open',{url:origin});
  const upload = await app.tool([{tool:'snapshot'},{tool:'upload',refName:'Upload sample',arguments:{file:'sample.txt'}}]);
  assert.match(upload,/uploaded/);
  await app.tool('open',{url:origin});
  const download = await app.tool([{tool:'snapshot'},{tool:'download',refName:'Download sample',arguments:{file:'downloaded.txt'}}]);
  assert.match(download,/downloaded.txt/);
  await app.control('take');
  const file = await app.control('get_file',{file:'downloaded.txt'});
  assert.equal(Buffer.from(file.data,'base64').toString(),'Synthetic browser transfer');
  for (const body of [
    {op:'get_file',token:app.token,file:'../profile/Cookies'},
    {op:'put_file',token:app.token,file:'sample.txt',data:Buffer.from('overwrite').toString('base64')},
    {op:'put_file',token:app.token,file:'too-big.bin',data:Buffer.alloc(2*1024*1024+1).toString('base64')},
  ]) {
    const response = await fetch(app.base+'/browser/control',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});
    assert.equal(response.status,400);
  }
  await app.control('resume');
  await app.request('/vm/stop',{}); await app.request('/vm/start',{});
  await app.control('take');
  const expired = await fetch(app.base+'/browser/control',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({op:'get_file',token:app.token,file:'sample.txt'})});
  assert.equal(expired.status,400);
  await app.control('resume');
});

test('takeover cancels active approvals; VM Stop and closed-tab recovery never resume stopped work', { timeout: 180000 }, async (t) => {
  const app = await context(t,'1');
  await app.tool('open',{url:origin});
  const run = await app.submit('browser '+JSON.stringify({tool:'snapshot'}));
  const permission = await until(async () => (await app.state()).permissions.find((p) => p.task_id===run.id),'No pending browser approval');
  await app.control('take');
  assert.equal((await app.done(run.id)).task.status,'interrupted');
  const oldApproval = await fetch(app.base+`/permissions/${permission.id}`,{method:'POST',headers:{'Content-Type':'application/json'},body:'{"allow":true}'});
  assert.equal(oldApproval.status,400);
  const queued = await app.submit('Queued while the owner is busy');
  const calls = app.provider.requests.length;
  await new Promise((resolve) => setTimeout(resolve,2200));
  assert.equal((await app.request('/vm/status')).browser_owner,true);
  assert.equal(app.provider.requests.length,calls);
  await app.request('/vm/stop',{});
  assert.equal((await app.done(queued.id)).task.status,'stopped');
  const event = await app.request('/intake/events',{source:'browser-proof',event_id:randomUUID(),content:'Wait until the owner explicitly starts dispatch'});
  const output = await promisify(execFile)(process.env.AYATI_TEST_BIN ?? join(root,'target/debug/ayati'),['browser','release'],{
    cwd:app.data,env:{PATH:process.env.PATH,AYATI_RUNTIME:'goose',AYATI_EXECUTION:'vm',AYATI_PORT:new URL(app.base).port,AYATI_DATA_DIR:app.data},
  });
  assert.match(output.stdout,/handed back/); assert.ok(!output.stdout.includes(app.token));
  assert.equal((await app.request('/browser/status')).owner,false);
  assert.equal((await app.request('/vm/status')).dispatch_paused,true);
  assert.equal((await app.state()).tasks.find((task)=>task.id===event.id).status,'queued');
  await app.request('/vm/start',{});
  assert.equal((await app.done(event.id)).task.status,'completed');
});

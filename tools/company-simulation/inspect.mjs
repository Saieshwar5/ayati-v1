import { chromium } from '../../web/node_modules/playwright-core/index.mjs';
import { readFile,writeFile } from 'node:fs/promises';
import { resolve,join } from 'node:path';
import assert from 'node:assert/strict';
const {directory}=JSON.parse(await readFile(resolve('.ayati-local/company-simulation/runs/latest.json')));
const c=JSON.parse(await readFile(join(directory,'manifest.json')));
const browser=await chromium.launch({headless:true,executablePath:'/usr/bin/chromium',args:['--no-sandbox']});
const context=await browser.newContext({viewport:{width:1366,height:900}});
await context.request.post(`${c.base}/api/identity/simulation/employees/0/login`,{headers:{Origin:c.base,'X-Ayati-Request':'identity'},data:{}});
const page=await context.newPage(),errors=[];page.on('pageerror',e=>errors.push(String(e)));
try{
 await page.goto(`${c.base}/w/${c.w.split('/').at(-1)}/?view=company`);
 await page.getByRole('heading',{name:'Simulation results'}).waitFor();await page.locator('.company-run summary').first().click();
 await page.getByText(/messages over/).first().waitFor();
 assert.match(await page.locator('.company-page').innerText(),/20\/20 accepted/);
 await page.screenshot({path:join(directory,'company-results.png')});
 await page.goto(`${c.base}/w/${c.w.split('/').at(-1)}/?view=chat&conversation=${c.channels.Operations}`);
 await page.getByRole('heading',{name:/^# Operations · Rehearsal/i}).waitFor();
 assert.ok(await page.getByRole('link',{name:'Switch employee',exact:true}).isVisible());
 await page.screenshot({path:join(directory,'company-chat-final.png')});
 await page.setViewportSize({width:320,height:568});
 assert.ok(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth));
 await page.screenshot({path:join(directory,'company-chat-narrow.png')});
 assert.deepEqual(errors,[]);
 await writeFile(join(directory,'inspection.json'),JSON.stringify({passed:true,report_visible:true,short_names:true,simulation_banner:true,narrow_chat:true,errors},null,2));
 console.log('Final results dashboard, channel names, simulation switcher and narrow Chat verified.');
}finally{await browser.close();}

import { chromium } from '../../web/node_modules/playwright-core/index.mjs';
import assert from 'node:assert/strict';
import { readFile,writeFile } from 'node:fs/promises';
import { resolve,join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { performance } from 'node:perf_hooks';
import { randomUUID } from 'node:crypto';
const {directory}=JSON.parse(await readFile(resolve(process.argv[2]??'.ayati-local/company-simulation/runs','latest.json')));
const c=JSON.parse(await readFile(join(directory,'manifest.json')));
const browser=await chromium.launch({headless:true,executablePath:'/usr/bin/chromium',args:['--no-sandbox','--use-fake-ui-for-media-stream','--use-fake-device-for-media-stream']});
const errors=[],results=[];
async function context(employee){const context=await browser.newContext({viewport:{width:1366,height:900},permissions:['microphone']});await context.request.post(`${c.base}/api/identity/simulation/employees/${employee}/login`,{headers:{Origin:c.base,'X-Ayati-Request':'identity'},data:{}});return context;}
const owner=await context(0),member=await context(7);
const p=await owner.newPage(),q=await member.newPage();
for(const page of [p,q])page.on('pageerror',e=>errors.push(String(e)));
async function visit(page,view){await page.goto(`${c.base}/w/${c.w.split('/').at(-1)}/?view=${view}`);}
try{
 await visit(p,`chat&conversation=${c.channels.Operations}`);await visit(q,`chat&conversation=${c.channels.Operations}`);
 await p.getByRole('textbox',{name:'Message',exact:true}).waitFor();await q.getByRole('textbox',{name:'Message',exact:true}).waitFor();
 const latencies=[];
 for(let i=0;i<4;i++){const text=`Browser delivery probe ${randomUUID()}`;await p.getByRole('textbox',{name:'Message',exact:true}).fill(text);const start=performance.now();await p.getByRole('button',{name:'Send message',exact:true}).click();await q.getByText(text,{exact:true}).waitFor();latencies.push(performance.now()-start);}
 results.push({name:'two-session message delivery',passed:true,samples_ms:latencies});
 await p.getByLabel('Choose attachment',{exact:true}).setInputFiles({name:'browser-order.txt',mimeType:'text/plain',buffer:Buffer.from('Browser upload: Northstar order, verified by Operations.')});
 await p.getByRole('button',{name:'Remove browser-order.txt'}).waitFor();await p.getByRole('textbox',{name:'Message',exact:true}).fill('Document sent from the browser');await p.getByRole('button',{name:'Send message',exact:true}).click();await q.getByRole('link',{name:'browser-order.txt',exact:true}).last().waitFor();results.push({name:'browser upload and recipient attachment',passed:true});
 await p.getByRole('button',{name:'Record voice',exact:true}).click();await p.getByRole('button',{name:'Stop recording',exact:true}).waitFor();await p.waitForTimeout(1200);await p.getByRole('button',{name:'Stop recording',exact:true}).click();await p.getByRole('button',{name:'Remove voice-message.webm'}).waitFor();await p.getByRole('textbox',{name:'Message',exact:true}).fill('Driver recording captured through the microphone control');await p.getByRole('button',{name:'Send message',exact:true}).click();await q.locator('audio[aria-label="voice-message.webm"]').last().waitFor();results.push({name:'microphone recording and audio element',passed:true});
 execFileSync('python',['tools/company-simulation/make-fixture.py',join(directory,'delivery-inspection.png')]);
 await p.getByLabel('Choose attachment',{exact:true}).setInputFiles(join(directory,'delivery-inspection.png'));
 await p.getByRole('button',{name:'Remove delivery-inspection.png'}).waitFor();await p.getByRole('textbox',{name:'Message',exact:true}).fill('Delivery inspection: the marked corner needs Operations review before Support closes the complaint.');await p.getByRole('button',{name:'Send message',exact:true}).click();
 const image=q.getByRole('img',{name:'delivery-inspection.png',exact:true});await image.scrollIntoViewIfNeeded();await q.waitForFunction(()=>document.querySelector('img[alt="delivery-inspection.png"]')?.naturalWidth===640);
 const audio=q.locator('audio[aria-label="voice-message.webm"]').last();await audio.scrollIntoViewIfNeeded();await audio.evaluate(async node=>{await node.play();});await q.waitForFunction(()=>[...document.querySelectorAll('audio')].some(a=>a.getAttribute('aria-label')==='voice-message.webm'&&a.currentTime>0.05));
 results.push({name:'image decode and recorded audio playback',passed:true});
 await p.screenshot({path:join(directory,'company-chat-desktop.png')});
 await visit(p,'home');await p.getByRole('heading',{name:'Work records',exact:true}).waitFor();await p.screenshot({path:join(directory,'workspace-work-desktop.png')});
 await visit(p,'feedback');await p.getByRole('heading',{name:'Feedback',exact:true}).waitFor();await p.getByRole('button',{name:'Report an issue'}).click();await p.getByLabel('Issue',{exact:true}).fill('Browser feedback journey');await p.getByLabel('Expected result').fill('An employee can report a problem without losing context.');await p.getByLabel('Actual result').fill('Report created during the automated browser rehearsal.');await p.getByRole('button',{name:'Save report'}).click();await p.getByRole('heading',{name:'Browser feedback journey'}).last().waitFor();results.push({name:'feedback submission through product UI',passed:true});
 for(const theme of ['light','dark']){await p.evaluate(theme=>{document.documentElement.dataset.theme=theme;},theme);for(const viewport of [{width:1366,height:900},{width:320,height:568}]){await p.setViewportSize(viewport);const fits=await p.evaluate(()=>document.documentElement.scrollWidth<=innerWidth);assert.ok(fits,`${theme} ${viewport.width} overflow`);await p.screenshot({path:join(directory,`feedback-${theme}-${viewport.width}.png`)});}}
 results.push({name:'feedback viewport containment, light/dark',passed:true});
 assert.deepEqual(errors,[]);
}catch(e){results.push({name:'browser journey',passed:false,error:String(e)});await p.screenshot({path:join(directory,'browser-failure.png')});process.exitCode=1;}
finally{await writeFile(join(directory,'browser-results.json'),JSON.stringify({results,errors},null,2));await browser.close();console.log(JSON.stringify({results,errors},null,2));}

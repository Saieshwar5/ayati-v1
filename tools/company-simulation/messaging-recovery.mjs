// Starts two temporary processes against the isolated company database; all data is retained.
import { chromium } from '../../web/node_modules/playwright-core/index.mjs';
import { readFile,writeFile,mkdir,copyFile,symlink } from 'node:fs/promises';
import { spawn } from 'node:child_process';
import { resolve,join } from 'node:path';
import { randomUUID } from 'node:crypto';
import assert from 'node:assert/strict';
const root=resolve('.ayati-local/company-simulation');
const {directory}=JSON.parse(await readFile(join(root,'runs/latest.json')));
const c=JSON.parse(await readFile(join(directory,'manifest.json')));
const evidence=join(directory,`messaging-recovery-${Date.now()}`);await mkdir(evidence);
const url=`postgresql://relay_app@localhost/relay_simulation_company?host=${root}/socket&port=55433`;
const servers=[],checks=[];
const wait=async(fn)=>{for(let i=0;i<160;i++){try{if(await fn())return;}catch{}await new Promise(r=>setTimeout(r,100));}throw new Error('Recovery condition timed out');};
async function start(port) {
 const data=join(root,`messaging-node-${port}`);await mkdir(data,{recursive:true});
 for(const file of ['SIMULATION','company.json'])await copyFile(join(root,'data',file),join(data,file));
 try{await symlink(join(root,'data/workspaces'),join(data,'workspaces'));}catch(e){if(e.code!=='EEXIST')throw e;}
 const process=spawn('target/debug/ayati',['simulate',data],{env:{...globalThis.process.env,AYATI_PORT:String(port),RELAY_SIMULATION_DATABASE_URL:url},stdio:['ignore','pipe','pipe']});
 let output='';process.stdout.on('data',chunk=>{output+=chunk;});process.stderr.on('data',chunk=>{output+=chunk;});
 servers.push(process);process.on('exit',()=>{void writeFile(join(evidence,`server-${port}.log`),output);});await wait(async()=>process.exitCode===null&&(await fetch(`http://127.0.0.1:${port}/api/identity/simulation`)).ok);
 return process;
}
async function stop(process) {process.kill('SIGINT');await wait(()=>process.exitCode!==null);}
const browser=await chromium.launch({headless:true,executablePath:'/usr/bin/chromium',args:['--no-sandbox']});
try{
 let first=await start(8877);await start(8878);
 const a=await browser.newContext(),b=await browser.newContext();
 for(const [context,port,num]of [[a,8877,0],[b,8878,3]])assert.ok((await context.request.post(`http://127.0.0.1:${port}/api/identity/simulation/employees/${num}/login`,{headers:{Origin:`http://127.0.0.1:${port}`,'X-Ayati-Request':'identity'},data:{}})).ok());
 const chat=randomUUID(),base='http://127.0.0.1:8877',headers={Origin:base,'X-Ayati-Request':'identity'};
 assert.ok((await a.request.post(`${base}${c.w}/chats`,{headers,data:{id:chat,name:'Cross-instance recovery '+Date.now(),kind:'channel',description:'Retained restart evidence',users:[c.employees[3].user.id],agents:[]}})).ok());
 const sender=await a.newPage(),receiver=await b.newPage();
 const path=`/w/${c.w.split('/').at(-1)}/?view=chat&conversation=${chat}`;
 await Promise.all([sender.goto(base+path),receiver.goto('http://127.0.0.1:8878'+path)]);
 for(const page of [sender,receiver])await page.getByText('Connected',{exact:true}).waitFor();
 await sender.getByRole('textbox',{name:'Message',exact:true}).fill('Cross-instance PostgreSQL notification');
 await sender.getByRole('button',{name:'Send message',exact:true}).click();
 await receiver.getByText('Cross-instance PostgreSQL notification',{exact:true}).waitFor();checks.push('Live message crosses application instances');
 await stop(first);
 await sender.getByRole('textbox',{name:'Message',exact:true}).fill('Saved while server is stopped');
 await sender.getByRole('button',{name:'Send message',exact:true}).click();
 await sender.getByText('◷ Checking delivery…',{exact:true}).waitFor();
 assert.ok(await sender.getByText('Cross-instance PostgreSQL notification',{exact:true}).isVisible());
 first=await start(8877);
 await receiver.getByText('Saved while server is stopped',{exact:true}).waitFor({timeout:20000});
 await wait(async()=>await sender.locator('[aria-label="Your pending message"]').count()===0);
 const result=await a.request.get(`${base}${c.w}/chats/${chat}/messages`);const data=await result.json();
 assert.equal(data.messages.filter(m=>m.content==='Saved while server is stopped').length,1);
 checks.push('Actual server shutdown and restart preserves outbox and sends once');
 await writeFile(join(evidence,'results.json'),JSON.stringify({checks,chat},null,2));console.log(JSON.stringify({evidence,checks},null,2));
}finally{
 await browser.close();
 for(const server of servers)if(server.exitCode===null)await stop(server);
}

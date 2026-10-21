import assert from 'node:assert/strict';
import { readFile,writeFile } from 'node:fs/promises';
import { join,resolve } from 'node:path';
import { execFileSync } from 'node:child_process';
import { performance } from 'node:perf_hooks';
import { clients,delay,randomUUID,until,distribution } from './client.mjs';
const root=resolve('.ayati-local/company-simulation/runs'),{directory}=JSON.parse(await readFile(join(root,'latest.json')));
const c=JSON.parse(await readFile(join(directory,'manifest.json'))),samples=[],errors=[],memory=[],accepted=[];
const {request}=clients(c.base,samples);
for(const e of c.employees)await request(e.number,`/api/identity/simulation/employees/${e.number}/login`,{});
let pid;try{pid=execFileSync('pgrep',['-f','^target/debug/ayati simulate /home/sai-eshwar/Projects/ayati/.ayati-local/company-simulation/data$'],{encoding:'utf8'}).trim();}catch{}
const start=performance.now(),duration=Number(process.argv[2]??90)*1000;
let collecting=true;
const monitor=(async()=>{while(collecting){if(pid){try{const status=await readFile(`/proc/${pid}/status`,'utf8');memory.push({at_ms:performance.now()-start,rss_kib:Number(status.match(/VmRSS:\s+(\d+)/)?.[1]),high_water_kib:Number(status.match(/VmHWM:\s+(\d+)/)?.[1])});}catch{}}await delay(1000);}})();
await Promise.all(c.employees.map(async e=>{
 let step=0;
 while(performance.now()-start<duration){
  const id=randomUUID(),agent=step%15===0?c.agents[e.number].id:undefined;
  const content=`${e.name} · order ${String(Math.floor(step/6)+1).padStart(3,'0')}: ${['reviewing customer requirements','checking quantities','waiting for a colleague’s confirmation','recording a clarification','confirming the handoff','reviewing the outcome'][step%6]}. ${agent?'Please acknowledge this responsibility.':''}`;
  try{await request(e.number,`${c.w}/chats/${c.channels[e.team]}/messages`,{id,content,agent},{label:'sustained-message-send'});accepted.push({id,chat:c.channels[e.team],agent});}
  catch(error){errors.push({employee:e.number,step,error:String(error)});}
  step++;await delay(700+(e.number%5)*70);
 }
}));
await until(()=>request(0,`${c.w}/metrics`),m=>m.pending===0,'sustained queue drain',60000);
collecting=false;await monitor;
const seen=new Map();
for(const chat of new Set(accepted.map(m=>m.chat))){let before;do{const h=await request(0,`${c.w}/chats/${chat}/messages${before?`?before=${before}`:''}`);for(const m of h.messages)seen.set(m.id,m);before=h.next_before;}while(before);}
const missing=accepted.filter(m=>!seen.has(m.id));
const agentReplies=[...seen.values()].filter(m=>m.kind==='agent'&&accepted.some(a=>a.id===m.thread));
const report={duration_ms:performance.now()-start,accepted:accepted.length,errors,missing:missing.length,expected_agent_replies:accepted.filter(m=>m.agent).length,agent_replies:agentReplies.length,failed_replies:agentReplies.filter(m=>m.state!=='complete').length,latency:distribution(samples.filter(s=>s.label==='sustained-message-send').map(s=>s.ms)),memory,queue_scope:'Cumulative workspace history, including deliberately paused recovery jobs; not a per-run latency distribution',queue:(await request(0,`${c.w}/metrics`)).queue};
await writeFile(join(directory,'sustained-load.json'),JSON.stringify(report,null,2));await writeFile(join(directory,'sustained-requests.jsonl'),samples.map(s=>JSON.stringify(s)).join('\n'));
console.log(JSON.stringify({...report,memory:{samples:memory.length,first:memory[0],last:memory.at(-1)}},null,2));
assert.equal(errors.length,0);assert.equal(missing.length,0);assert.equal(report.failed_replies,0);assert.equal(report.agent_replies,report.expected_agent_replies);

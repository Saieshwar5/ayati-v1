// Twenty real SSE subscribers and durable recipient acknowledgements; all records retained.
import { readFile,writeFile } from 'node:fs/promises';
import { resolve,join } from 'node:path';
import assert from 'node:assert/strict';
import { clients,randomUUID,delay,until,distribution } from './client.mjs';
const {directory}=JSON.parse(await readFile(resolve('.ayati-local/company-simulation/runs/latest.json')));
const c=JSON.parse(await readFile(join(directory,'manifest.json'))),samples=[];
const {request,sessions}=clients(c.base,samples);
for(const e of c.employees)await request(e.number,`/api/identity/simulation/employees/${e.number}/login`,{});
const chat=randomUUID();await request(0,`${c.w}/chats`,{id:chat,name:`Live delivery load ${Date.now()}`,kind:'channel',description:'Twenty connected employee clients; retained measurement',users:c.employees.map(e=>e.user.id),agents:[]});
const abort=new AbortController(),ready=new Set(),observed=c.employees.map(()=>new Map()),sent=new Map(),latencies=[];
const readers=c.employees.map(async e=>{
 const response=await fetch(`${c.base}${c.w}/message-events`,{headers:{Cookie:sessions.get(e.number)},signal:abort.signal});assert.ok(response.ok);
 let buffer='';
 for await(const chunk of response.body){buffer+=Buffer.from(chunk).toString();let end;
  while((end=buffer.indexOf('\n\n'))>=0){const frame=buffer.slice(0,end);buffer=buffer.slice(end+2);const data=frame.split('\n').find(line=>line.startsWith('data: '));if(!data)continue;
   const payload=JSON.parse(data.slice(6));ready.add(e.number);
   for(const event of payload.events??[])if(event.chat===chat&&event.message&&!observed[e.number].has(event.message.id)){observed[e.number].set(event.message.id,event.message);if(sent.has(event.message.id))latencies.push(performance.now()-sent.get(event.message.id));}
  }
 }
}).map(p=>p.catch(error=>{if(!abort.signal.aborted)throw error;}));
try{
 await until(async()=>ready.size,n=>n===20,'twenty stream connections');
 for(let round=0;round<5;round++){
  await Promise.all(c.employees.map(async e=>{const id=randomUUID();sent.set(id,performance.now());await request(e.number,`${c.w}/chats/${chat}/messages`,{id,content:`Live load round ${round+1}: ${e.name}`,attachments:[]},{label:'live-send'});}));
  await delay(150);
 }
 await until(async()=>observed.every(map=>[...sent.keys()].every(id=>map.has(id))),Boolean,'all recipients receive all messages',60000);
 await Promise.all(c.employees.map(e=>request(e.number,`${c.w}/chats/${chat}/receipts`,{ids:[...sent.keys()],seen:false},{label:'receipt-batch'})));
 const page=await request(0,`${c.w}/chats/${chat}/messages`);
 assert.ok(page.messages.filter(m=>m.author===c.employees[0].user.id).every(m=>m.delivered===19&&m.seen===0));
 const report={chat,employees:20,messages:sent.size,deliveries:observed.reduce((n,map)=>n+map.size,0),missing:0,send:distribution(samples.filter(s=>s.label==='live-send').map(s=>s.ms)),delivery:distribution(latencies),receipts_verified:19};
 const path=join(directory,`messaging-load-${Date.now()}.json`);await writeFile(path,JSON.stringify(report,null,2));console.log(JSON.stringify({path,...report},null,2));
}finally{abort.abort();await Promise.allSettled(readers);}

import assert from 'node:assert/strict';
import { readFile,writeFile } from 'node:fs/promises';
import { resolve,join } from 'node:path';
import { clients,randomUUID,until } from './client.mjs';
const {directory}=JSON.parse(await readFile(resolve(process.argv[3]??'.ayati-local/company-simulation/runs','latest.json')));
const c=JSON.parse(await readFile(join(directory,'manifest.json')));
const {request}=clients(c.base,[]);
await request(0,'/api/identity/simulation/employees/0/login',{});
const path=join(directory,'recovery-requests.json');
if(process.argv[2]==='prepare'){
 await request(0,'/api/identity/simulation/dispatch',{enabled:false});
 const ids=[];
 for(const e of c.employees){const id=randomUUID();await request(0,`${c.w}/chats/${c.channels[e.team]}/messages`,{id,content:'Restart recovery request thread.'});await request(0,`${c.w}/chats/${c.channels[e.team]}/messages`,{id:randomUUID(),thread:id,content:`Please retain this handoff across server restart for ${e.name}.`,agent:c.agents[e.number].id});ids.push({id,chat:c.channels[e.team]});}
 assert.equal((await request(0,`${c.w}/metrics`)).pending,20);
 await writeFile(path,JSON.stringify(ids,null,2));console.log('Prepared 20 persisted jobs with dispatch paused. Restart the simulation server, then verify.');
}else{
 const ids=JSON.parse(await readFile(path));
 await until(()=>request(0,`${c.w}/metrics`),m=>m.pending===0,'recovered job completion',30000);
 for(const {id,chat}of ids){const h=await request(0,`${c.w}/chats/${chat}/messages`);assert.equal(h.messages.filter(m=>m.id===id).length,1);const replies=h.messages.filter(m=>m.thread===id&&m.kind==='agent');assert.equal(replies.length,1);assert.equal(replies[0].state,'complete');}
 const result={passed:true,description:'Twenty accepted jobs remained queued across a real server restart and each produced exactly one completed reply.',checked_at:new Date().toISOString()};await writeFile(join(directory,'recovery-result.json'),JSON.stringify(result,null,2));console.log(result.description);
}

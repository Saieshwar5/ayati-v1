// Keep human-readable rehearsal names without replacing any conversations or messages.
import { readdir,readFile } from 'node:fs/promises';
import { resolve,join } from 'node:path';
import { clients } from './client.mjs';
const root=resolve('.ayati-local/company-simulation/runs');
const {request}=clients('http://127.0.0.1:8876',[]);
await request(0,'/api/identity/simulation/employees/0/login',{});
let number=0;
for(const entry of (await readdir(root,{withFileTypes:true})).filter(e=>e.isDirectory()).sort((a,b)=>a.name.localeCompare(b.name))){
 const path=join(root,entry.name,'manifest.json');let c;try{c=JSON.parse(await readFile(path));}catch{continue;}number++;
 for(const [name,id]of Object.entries(c.channels))await request(0,`${c.w}/chats/${id}/settings`,{name:`${name} · Rehearsal ${number}`,visibility:name==='Company'?'public':'private',announcements:name==='Company'});
}
console.log(`Renamed ${number} rehearsals; every conversation ID and message is retained.`);

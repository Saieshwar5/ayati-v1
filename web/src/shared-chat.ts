import { identityRequest, workspaceId, workspaceUrl, canManageResources, storageScope } from './workspace-context';
import { element } from './view';
import './shared-chat.css';
export type Directory = { users: {id:string;name:string;role:string}[]; agents:{id:string;name:string}[]; grants:{user:string;agent:string}[];role:string };
type Conversation = {id:string;name:string};
type Message = {id:string;author:string;name:string|null;kind:string;content:string;state:string;at:number};
const base = () => `/workspaces/${workspaceId}`;
export async function installSharedChat(): Promise<void> {
  document.title = 'Chat · Relay';
  const root = document.querySelector<HTMLDivElement>('#app')!; root.className = 'shared-chat';
  const sidebar = element('aside', 'shared-sidebar'); sidebar.setAttribute('aria-label', 'Conversations');
  const title = element('h1', '', 'Chat'); const add = element('button', 'files-button', 'New chat');
  const search = element('input'); search.type = 'search'; search.placeholder = 'Search conversations'; search.setAttribute('aria-label','Search conversations');
  const list = element('div'); const privateAgents = element('div'); sidebar.append(title,add,search,list,privateAgents);
  const main = element('section', 'shared-main'); const heading = element('header', 'shared-heading');
  const name = element('h2', '', 'Your team, together'); const hint = element('p', 'page-subtitle', 'Start a conversation with people, agents, or both.'); heading.append(name,hint);
  const messages = element('div', 'shared-messages'); messages.setAttribute('role','log'); messages.setAttribute('aria-label','Messages');
  const status = element('p', 'shared-status'); status.setAttribute('role','status');
  const form = element('form', 'shared-composer');
  const target = element('select'); target.setAttribute('aria-label','Send to conversation or ask an agent');
  const input = element('textarea'); input.placeholder = 'Write a message…'; input.setAttribute('aria-label','Message'); input.required = true; input.maxLength=12000;
  const send = element('button', 'send-button', '↑'); send.setAttribute('aria-label','Send message');
  form.append(target,input,send); form.hidden = true; main.append(heading,messages,status,form); root.append(sidebar,main);
  let directory = await identityRequest<Directory>(`${base()}/chat-directory`);
  let selected = new URLSearchParams(location.search).get('conversation') ?? '';
  let conversations: Conversation[] = []; let fingerprint = ''; let polling = false; let sending = false;
  let attempt: {id:string;content:string;agent?:string} | undefined;
  const request = <T>(path:string,body?:unknown) => identityRequest<T>(`${base()}/chats${path}`,body);
  function drawList(): void {
    list.replaceChildren();
    for (const chat of conversations.filter(c=>c.name.toLowerCase().includes(search.value.toLowerCase()))) {
      const link=element('a',`shared-contact ${chat.id===selected?'selected':''}`,chat.name); link.href=workspaceUrl(`/?view=chat&conversation=${chat.id}`); list.append(link);
    }
    if (!conversations.length) list.append(element('p','page-subtitle','No conversations yet.'));
  }
  search.oninput=drawList;
  if (canManageResources()) {
    privateAgents.append(element('h2','','Private agent chats'));
    for (const agent of directory.agents) { const link=element('a','shared-contact',agent.name); link.href=workspaceUrl(`/?agent=${agent.id}`); privateAgents.append(link); }
  }
  function draftKey():string{return storageScope(`relay.chat.draft.${selected}`);}
  try {input.value=sessionStorage.getItem(draftKey())??'';} catch { /* Optional local draft. */ }
  input.oninput=()=>{try{sessionStorage.setItem(draftKey(),input.value);}catch{/* Optional. */}};
  async function refresh():Promise<void> {
    if(polling)return; polling=true;
    try {
      conversations=(await request<{chats:Conversation[]}>('')).chats; drawList();
      if(!selected)return;
      const current=conversations.find(c=>c.id===selected); if(!current)throw new Error('This conversation is no longer available.');
      const data=await request<{messages:Message[];agents:string[]}>(`/${selected}/messages`);
      name.textContent=current.name; hint.textContent='Shared messages only. For business tools, use a private agent chat.';
      form.hidden=false;
      const chosen=target.value; target.replaceChildren(new Option('Message everyone',''));
      for(const id of data.agents){const agent=directory.agents.find(a=>a.id===id);if(agent)target.append(new Option(`Ask ${agent.name}`,id));}
      target.value=chosen; if(target.selectedIndex<0)target.selectedIndex=0;
      const next=JSON.stringify(data.messages);
      if(next!==fingerprint){
        const bottom=messages.scrollHeight-messages.scrollTop-messages.clientHeight<100;
        messages.replaceChildren();
        for(const message of data.messages){
          const card=element('article',`shared-message ${message.kind}`);
          const author=message.kind==='agent' ? directory.agents.find(a=>a.id===message.author)?.name??'Agent' : message.name??'Former member';
          card.append(element('strong','',`${author}${message.kind==='agent'?' · Agent':''}`),element('p','',message.content||'Preparing a reply…'));
          const time=element('small','',`${new Date(message.at*1000).toLocaleTimeString([], {hour:'2-digit',minute:'2-digit'})} · ${message.state}`);card.append(time);messages.append(card);
        }
        if(bottom||!fingerprint)messages.scrollTop=messages.scrollHeight; fingerprint=next;
      }
      status.textContent='';
    }catch(error){status.textContent=(error as Error).message;form.hidden=true;messages.replaceChildren();fingerprint='';}
    finally{polling=false;}
  }
  form.onsubmit=async event=>{
    event.preventDefault();if(sending||!selected)return;sending=true;send.disabled=true;
    if(!attempt||attempt.content!==input.value||attempt.agent!==(target.value||undefined)) attempt={id:crypto.randomUUID(),content:input.value,agent:target.value||undefined};
    try{await request(`/${selected}/messages`,attempt);input.value='';input.dispatchEvent(new Event('input'));attempt=undefined;await refresh();}
    catch(error){status.textContent=(error as Error).message;}
    finally{sending=false;send.disabled=false;}
  };
  const dialog=element('dialog','shared-new');dialog.setAttribute('aria-label','New conversation');
  const setup=element('form');const close=element('button','text-button','Cancel');close.type='button';close.onclick=()=>dialog.close();
  const label=element('label','','Conversation name');const chatName=element('input');chatName.required=true;chatName.maxLength=100;label.append(chatName);
  const choices=element('div');const error=element('p');error.setAttribute('role','status');const submit=element('button','files-button','Create conversation');
  setup.append(element('h2','','New conversation'),label,choices,error,submit,close);dialog.append(setup);document.body.append(dialog);
  add.onclick=async()=>{
    try{directory=await identityRequest<Directory>(`${base()}/chat-directory`);choices.replaceChildren();
      for(const [kind,items] of [['user',directory.users],['agent',directory.agents]] as const){
        choices.append(element('h3','',kind==='user'?'People':'Agents'));
        for(const item of items){const label=element('label','shared-choice');const check=element('input');check.type='checkbox';check.name=kind;check.value=item.id;label.append(check,document.createTextNode(item.name));choices.append(label);}
      }error.textContent='';dialog.showModal();chatName.focus();
    }catch(e){status.textContent=(e as Error).message;}
  };
  setup.onsubmit=async event=>{
    event.preventDefault();submit.disabled=true;const values=new FormData(setup);
    try{const created=await request<{id:string}>('',{id:crypto.randomUUID(),name:chatName.value,users:values.getAll('user'),agents:values.getAll('agent')});location.assign(workspaceUrl(`/?view=chat&conversation=${created.id}`));}
    catch(e){error.textContent=(e as Error).message;submit.disabled=false;}
  };
  await refresh();setInterval(()=>{if(!document.hidden)void refresh();},2500);
}

import { identityRequest, workspaceId } from './workspace-context';
import { element } from './view';
import type { Directory } from './shared-chat';
export async function installUserAgentAccess(root:HTMLElement):Promise<void> {
  const base=`/workspaces/${workspaceId}`;
  const data=await identityRequest<Directory>(`${base}/chat-directory`);
  if(data.role!=='Owner')return;
  const panel=element('section','access-panel');panel.append(element('h2','','Agent access'),element('p','access-help','Choose who can ask each agent for replies in shared Chat. This does not grant configuration access, private history, or business tools.'));
  const status=element('p');status.setAttribute('role','status');panel.append(status);
  for(const user of data.users.filter(u=>u.role!=='Owner')){
    const row=element('div','access-person');row.append(element('strong','',user.name));
    const options=element('div','access-actions');
    for(const agent of data.agents){
      const label=element('label','access-field',agent.name);const checkbox=element('input');checkbox.type='checkbox';checkbox.checked=data.grants.some(g=>g.user===user.id&&g.agent===agent.id);
      checkbox.setAttribute('aria-label',`Allow ${user.name} to chat with ${agent.name}`);
      checkbox.onchange=async()=>{checkbox.disabled=true;try{await identityRequest(`${base}/chat-grants`,{user:user.id,agent:agent.id,allowed:checkbox.checked});status.textContent='Access updated.';}catch(e){checkbox.checked=!checkbox.checked;status.textContent=(e as Error).message;}finally{checkbox.disabled=false;}};
      label.prepend(checkbox);options.append(label);
    }
    row.append(options);panel.append(row);
  }
  root.append(panel);
}

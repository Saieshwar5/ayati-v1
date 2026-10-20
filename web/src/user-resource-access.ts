import { identityRequest, workspaceId } from './workspace-context';
import { element } from './view';
import type { Directory } from './shared-chat';
type Resource = {id:string;name?:string;active_revision:number|null;active?:{definition:{title:string}}|null};
type Access = {apps:Resource[];panels:Resource[];grants:{user:string;kind:string;resource:string;access:string}[]};
export async function installResourceAccess(root:HTMLElement):Promise<void>{
  const base=`/workspaces/${workspaceId}`;
  const [directory,data]=await Promise.all([identityRequest<Directory>(`${base}/chat-directory`),identityRequest<Access>(`${base}/resource-access`)]);
  const panel=element('section','access-panel');panel.append(element('h2','','Apps and Home panels'),element('p','access-help','Share published Apps and panels with individual members. App data access does not grant release, configuration, or agent permissions.'));
  const status=element('p');status.setAttribute('role','status');panel.append(status);
  let count=0;
  for(const [kind,resources] of [['app',data.apps],['panel',data.panels.filter(p=>p.active)]] as const){
    for(const resource of resources){
      count++;const section=element('section');section.append(element('h3','',resource.name??resource.active?.definition.title??'Home panel'));
      for(const user of directory.users.filter(u=>u.role!=='Owner')){
        const row=element('label','access-person',user.name);const select=element('select');select.setAttribute('aria-label',`${user.name} access to ${resource.name??'panel'}`);
        select.append(new Option('No access',''),new Option('Read only','read'));if(kind==='app')select.append(new Option('Read, edit and delete records','write'));
        select.value=data.grants.find(g=>g.user===user.id&&g.kind===kind&&g.resource===resource.id)?.access??'';
        let saved=select.value;select.onchange=async()=>{select.disabled=true;try{await identityRequest(`${base}/resource-access`,{user:user.id,kind,resource:resource.id,access:select.value||null});saved=select.value;status.textContent='Resource access updated.';}catch(e){select.value=saved;status.textContent=(e as Error).message;}finally{select.disabled=false;}};
        row.append(select);section.append(row);
      }panel.append(section);
    }
  }
  if(!count)panel.append(element('p','access-help','Publish an App or keep a panel on Home to share it here.'));
  root.append(panel);
}

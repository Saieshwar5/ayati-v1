import { identityRequest, workspaceId } from './workspace-context.ts';
import { element } from './view.ts';
type Resource = { id: string; name?: string; active_revision: number | null; active?: { definition: { title: string } } | null };
type Access = { apps: Resource[]; panels: Resource[]; grants: { user: string; kind: string; resource: string; access: string }[] };

export async function installResourceAccess(root: HTMLElement, user: string, status: HTMLElement, setBusy: (busy: boolean) => void): Promise<void> {
  const base = `/workspaces/${workspaceId}`;
  const data = await identityRequest<Access>(`${base}/resource-access`);
  root.replaceChildren(element('p', 'ui-help', 'Share Apps and Home panels with this person. Record access does not grant app configuration or release permissions. Changes save immediately.'));
  let count = 0;
  for (const [kind, resources] of [['app', data.apps], ['panel', data.panels.filter(p => p.active)]] as const) {
    if (resources.length) root.append(element('h3', '', kind === 'app' ? 'Apps' : 'Home panels'));
    for (const resource of resources) {
      count++;
      const row = element('label', 'ui-resource', resource.name ?? resource.active?.definition.title ?? 'Home panel');
      const select = element('select', 'ui-select');
      select.append(new Option('No access', ''), new Option('Read only', 'read'));
      if (kind === 'app') select.append(new Option('Read, edit and delete records', 'write'));
      select.value = data.grants.find(g => g.user === user && g.kind === kind && g.resource === resource.id)?.access ?? '';
      let saved = select.value;
      select.onchange = async () => {
        select.disabled = true; setBusy(true); status.textContent = 'Saving resource access…';
        try { await identityRequest(`${base}/resource-access`, { user, kind, resource: resource.id, access: select.value || null }); saved = select.value; status.textContent = 'Resource access saved.'; }
        catch (error) { select.value = saved; status.textContent = (error as Error).message; }
        finally { select.disabled = false; setBusy(false); }
      };
      row.append(select); root.append(row);
    }
  }
  if (!count) root.append(element('p', 'ui-empty', 'Publish an App or keep a panel on Home to share it here.'));
}

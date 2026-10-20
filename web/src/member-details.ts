import { identityRequest, workspaceId } from './workspace-context.ts';
import { element } from './view.ts';
import { createDialog, installTabs, uiButton } from './app-ui.ts';
import { installUserAgentAccess } from './user-agent-access.ts';
import { installResourceAccess } from './user-resource-access.ts';
import type { Member } from './people.ts';

export function openMember(member: Member, viewerRole: string, refresh: () => Promise<void>): void {
  const sheet = createDialog('Member details', true);
  sheet.dialog.addEventListener('close', () => {
    [...document.querySelectorAll<HTMLButtonElement>('[data-member]')].find(button => button.dataset.member === member.id)?.focus();
  });
  sheet.body.append(element('p', 'ui-member-email', member.email));
  const rolePanel = element('section', 'ui-form');
  rolePanel.append(element('p', 'ui-help', 'Workspace membership and access to business resources are managed separately.'));
  const roleField = element('label', 'ui-field', 'Workspace role');
  const role = element('select');
  for (const name of member.role === 'Owner' ? ['Owner'] : ['Member', 'Admin']) role.append(new Option(name, name));
  role.value = member.role; role.disabled = viewerRole !== 'Owner' || member.role === 'Owner';
  roleField.append(role); rolePanel.append(roleField);
  rolePanel.append(element('p', 'ui-help', member.role === 'Owner'
    ? 'The owner manages this workspace and its resources.'
    : 'Admins can invite and remove members. Only the owner manages agents and shares business resources.'));
  let busy = false;
  async function change(nextRole: string | null): Promise<void> {
    if (busy) return; busy = true; sheet.setBusy(true);
    const controls = [...rolePanel.querySelectorAll<HTMLButtonElement | HTMLSelectElement>('button,select')];
    controls.forEach(c => { c.disabled = true; });
    try {
      await identityRequest(`/workspaces/${workspaceId}/members/${member.id}`, { role: nextRole });
      if (nextRole === null) sheet.dialog.close();
      else { member.role = nextRole; sheet.status.textContent = 'Workspace role updated.'; }
      try { await refresh(); } catch { sheet.status.textContent = 'Saved. Reload Users to refresh the list.'; }
    } catch (error) { role.value = member.role; sheet.status.textContent = (error as Error).message; }
    finally {
      busy = false; sheet.setBusy(false); controls.forEach(c => { c.disabled = false; });
      role.disabled = viewerRole !== 'Owner' || member.role === 'Owner';
    }
  }
  if (viewerRole === 'Owner' && member.role !== 'Owner') {
    rolePanel.append(uiButton('Save role', () => { void change(role.value); }, true));
  }
  if (member.role !== 'Owner' && (viewerRole === 'Owner' || (viewerRole === 'Admin' && member.role === 'Member'))) {
    const remove = uiButton('Remove from workspace', () => {
      if (confirm(`Remove ${member.email} from this workspace? Business records remain.`)) void change(null);
    });
    remove.classList.add('danger'); rolePanel.append(remove);
  }
  const sections = [{ title: 'Membership', content: rolePanel }];
  if (viewerRole === 'Owner' && member.role !== 'Owner') {
    const agents = element('section'), resources = element('section');
    sections.push({ title: 'Agents', content: agents }, { title: 'Apps & panels', content: resources });
    const load = (node: HTMLElement, perform: () => Promise<void>): void => {
      node.textContent = 'Loading access…';
      void perform().catch(error => {
        node.replaceChildren(element('p', 'ui-help', error.message), uiButton('Retry', () => load(node, perform)));
      });
    };
    load(agents, () => installUserAgentAccess(agents, member.id, sheet.status, sheet.setBusy));
    load(resources, () => installResourceAccess(resources, member.id, sheet.status, sheet.setBusy));
  }
  installTabs(sheet.body, sections); sheet.show();
}

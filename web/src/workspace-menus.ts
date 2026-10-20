import { currentWorkspace, workspaceSession, workspaceId, workspaceUrl, identityRequest } from './workspace-context.ts';
import { element } from './view.ts';

function initials(value: string): string {
  return value.split(/[\s@._-]+/).filter(Boolean).slice(0, 2).map(part => Array.from(part)[0]).join('').toUpperCase() || 'W';
}

function menu(id: string, label: string, text: string): { trigger: HTMLButtonElement; panel: HTMLDivElement } {
  const trigger = element('button', 'workspace-menu-trigger', text);
  trigger.type = 'button'; trigger.title = label;
  trigger.setAttribute('aria-label', label);
  trigger.setAttribute('aria-expanded', 'false'); trigger.setAttribute('aria-controls', id);
  const panel = element('div', `workspace-menu ${id}`);
  panel.id = id; panel.popover = 'auto'; panel.setAttribute('aria-label', label);
  trigger.popoverTargetElement = panel;
  panel.addEventListener('toggle', () => trigger.setAttribute('aria-expanded', String(panel.matches(':popover-open'))));
  document.body.append(panel);
  return { trigger, panel };
}

function link(label: string, href: string): HTMLAnchorElement {
  const anchor = element('a', 'workspace-menu-action', label); anchor.href = href; return anchor;
}

export function installWorkspaceMenus(rail: HTMLElement): void {
  const workspace = currentWorkspace();
  if (!workspaceId || !workspace) return;
  const list = element('nav', 'workspace-menu-list'); list.setAttribute('aria-label', 'Choose workspace');
  for (const entry of workspaceSession?.workspaces ?? []) {
    const choice = link(entry.name, `/w/${entry.id}/`);
    if (entry.id === workspaceId) {
      choice.setAttribute('aria-current', 'true');
      choice.append(element('small', '', 'Current'));
    }
    list.append(choice);
  }

  const email = workspaceSession?.user?.email ?? '';
  const profile = menu('workspace-profile', `${workspace.name} · Your account`, '');
  const avatar = element('span', 'workspace-profile-avatar', initials(email || 'You'));
  const indicator = element('span', 'workspace-profile-workspace', initials(workspace.name));
  avatar.setAttribute('aria-hidden', 'true'); indicator.setAttribute('aria-hidden', 'true');
  profile.trigger.append(avatar, indicator);
  profile.trigger.classList.add('workspace-profile-trigger'); rail.append(profile.trigger);
  profile.panel.append(element('h2', '', 'Your account'), element('p', 'workspace-menu-email', email), element('p', 'workspace-menu-role', `${workspace.name} · ${workspace.role}`), element('hr'), element('h2', '', 'Workspaces'), list, link('Manage or create workspaces', '/?view=account&choose=1'), element('hr'), link('My account', workspaceUrl('/?view=account')), link('Appearance', workspaceUrl('/?view=settings#theme')));
  const logout = element('button', 'workspace-menu-action', 'Sign out'); logout.type = 'button';
  const status = element('p', 'workspace-menu-status'); status.setAttribute('role', 'status');
  logout.onclick = async () => {
    logout.disabled = true; status.textContent = '';
    try { await identityRequest('/logout', {}); location.assign('/?view=account'); }
    catch { status.textContent = 'Sign-out failed. Please try again.'; logout.disabled = false; }
  };
  profile.panel.append(element('hr'), logout, status);
}

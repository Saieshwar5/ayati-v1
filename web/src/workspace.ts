import './workspace-menus.css';
import { installWorkspaceMenus } from './workspace-menus';
import { workspaceUrl, workspaceId, canManageResources } from './workspace-context';
import { brand } from './brand';
import { element } from './view';
import { openGuide } from './guide';

export type WorkspacePage = 'home' | 'apps' | 'chat' | 'agents' | 'settings' | 'people' | 'account';
const icons: Record<WorkspacePage, string> = {
  home: '<path d="m3 10 9-7 9 7v10H3Z"/><path d="M9 20v-7h6v7"/>',
  apps: '<rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/>',
  chat: '<path d="M21 11a8 8 0 0 1-8 8H5l-3 3V11a9 9 0 0 1 19 0Z"/><path d="M7 11h10"/>',
  agents: '<rect x="4" y="7" width="16" height="14" rx="4"/><path d="M12 3v4M1 12v4m22-4v4M8 12v2m8-2v2m-8 3h8"/>',
  people: '<circle cx="8" cy="7" r="3"/><path d="M2 21v-4a6 6 0 0 1 12 0v4m2-17a3 3 0 0 1 0 6m2 4a5 5 0 0 1 4 5v2"/>',
  account: '<circle cx="12" cy="8" r="4"/><path d="M4 22v-3a8 8 0 0 1 16 0v3"/>',
  settings: '<path d="M3 6h18M3 12h18M3 18h18"/><circle cx="8" cy="6" r="2"/><circle cx="16" cy="12" r="2"/><circle cx="9" cy="18" r="2"/>',
};

export function installWorkspace(page: WorkspacePage): void {
  if (new URLSearchParams(location.search).get('guide') === '1' && window.self !== window.top) document.documentElement.classList.add('guide-mode');
  const app = document.querySelector<HTMLDivElement>('#app')!;
  const shell = element('div', 'workspace-shell');
  const rail = element('aside', 'workspace-rail');
  const nav = element('nav'); nav.setAttribute('aria-label', 'Workspace');
  for (const name of (workspaceId ? ['home', 'apps', 'chat', 'agents', 'people', 'settings'] : ['home', 'apps', 'chat', 'agents', 'settings']) as WorkspacePage[]) {
    const link = element('a', `workspace-nav nav-${name}`);
    link.href = workspaceUrl(`/?view=${name}`);
    link.innerHTML = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true">${icons[name]}</svg><span>${name[0].toUpperCase() + name.slice(1)}</span>`;
    if (name === page) link.setAttribute('aria-current', 'page');
    link.title = name === 'people' ? 'Users' : name[0].toUpperCase() + name.slice(1);
    link.querySelector('span')!.textContent = link.title;
    nav.append(link);
  }
  const help = element('button', 'workspace-help', '?'); help.setAttribute('aria-label', 'Ask Relay'); help.title = 'Ask Relay'; help.onclick = openGuide; help.hidden = !canManageResources();
  const logo = element('a', 'workspace-brand'); logo.href = workspaceUrl('/'); logo.setAttribute('aria-label', 'Relay home');
  logo.append(brand('', true));
  rail.append(logo, nav, help);
  if (workspaceId) installWorkspaceMenus(rail);
  else rail.append(element('span', 'workspace-owner', 'You'));
  const body = element('div', 'workspace-body');
  app.before(shell); body.append(app); shell.append(rail, body);
  document.documentElement.dataset.page = page;
  try {
    const preference = localStorage.getItem('ayati.theme');
    if (preference === 'light' || preference === 'dark') document.documentElement.dataset.theme = preference;
  } catch { /* System preference is the fallback. */ }
}

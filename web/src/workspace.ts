import { element } from './view';
import { openGuide } from './guide';

export type WorkspacePage = 'home' | 'apps' | 'agents' | 'settings';
const icons: Record<WorkspacePage, string> = {
  home: '<path d="m3 10 9-7 9 7v10H3Z"/><path d="M9 20v-7h6v7"/>',
  apps: '<rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/>',
  agents: '<rect x="4" y="7" width="16" height="14" rx="4"/><path d="M12 3v4M1 12v4m22-4v4M8 12v2m8-2v2m-8 3h8"/>',
  settings: '<path d="M3 6h18M3 12h18M3 18h18"/><circle cx="8" cy="6" r="2"/><circle cx="16" cy="12" r="2"/><circle cx="9" cy="18" r="2"/>',
};

export function installWorkspace(page: WorkspacePage): void {
  if (new URLSearchParams(location.search).get('guide') === '1' && window.self !== window.top) document.documentElement.classList.add('guide-mode');
  const app = document.querySelector<HTMLDivElement>('#app')!;
  const shell = element('div', 'workspace-shell');
  const rail = element('aside', 'workspace-rail');
  const nav = element('nav'); nav.setAttribute('aria-label', 'Workspace');
  for (const name of ['home', 'apps', 'agents', 'settings'] as const) {
    const link = element('a', `workspace-nav nav-${name}`);
    link.href = name === 'agents' ? '/?agent=local-agent' : `/?view=${name}`;
    link.innerHTML = `<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true">${icons[name]}</svg><span>${name[0].toUpperCase() + name.slice(1)}</span>`;
    if (name === page) link.setAttribute('aria-current', 'page');
    link.title = name[0].toUpperCase() + name.slice(1);
    nav.append(link);
  }
  const help = element('button', 'workspace-help', '?'); help.setAttribute('aria-label', 'Ask Ayati'); help.title = 'Ask Ayati'; help.onclick = openGuide;
  rail.append(nav, help, element('span', 'workspace-owner', 'You'));
  const body = element('div', 'workspace-body');
  app.before(shell); body.append(app); shell.append(rail, body);
  document.documentElement.dataset.page = page;
  try {
    const preference = localStorage.getItem('ayati.theme');
    if (preference === 'light' || preference === 'dark') document.documentElement.dataset.theme = preference;
  } catch { /* System preference is the fallback. */ }
}

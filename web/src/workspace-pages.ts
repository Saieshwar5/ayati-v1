import { currentView } from './view-lifetime';
import { workspaceId, workspaceUrl, canManageResources } from './workspace-context';
import { installSetup } from './setup';
import { installConnections } from './connections';
import { installApps } from './apps';
import { element } from './view';
import { installTabs } from './app-ui';

export function installWorkspacePage(page: 'apps' | 'settings'): void {
  const view = currentView();
  document.title = `${page === 'apps' ? 'Apps' : 'Settings'} · Cuckoo`;
  if (page === 'apps') { installApps(); return; }
  const app = view.querySelector('#app')!;
  app.innerHTML = `<main class="desk-page settings-page"><header class="desk-toolbar"><div class="desk-title"><h1>Settings</h1><p>Make Cuckoo work the way you do.</p></div></header><div class="desk-content settings-content"></div></main>`;
  const content = app.querySelector<HTMLElement>('.settings-content')!;
  const appearance = element('section', 'settings-section');
  appearance.innerHTML = `<h2>Appearance</h2><p class="page-subtitle">Choose how Cuckoo looks on this device.</p><label class="settings-row">Theme<select id="theme"><option value="system">Use system setting</option><option value="light">Light</option><option value="dark">Dark</option></select></label>`;
  const sections = [{ title: 'Appearance', content: appearance }];
  const connections = element('section', 'settings-section'); connections.id = 'business-connections';
  const setup = element('section', 'settings-section');
  if (canManageResources()) {
    setup.innerHTML = `<h2>Local setup</h2><p id="setup-status" class="ui-help" role="status">Checking connection…</p><div id="setup-facts"></div><p class="ui-help">Provider credentials are configured locally. Restart Cuckoo after changing configuration.</p>`;
    sections.push({ title: 'Connections', content: connections }, { title: 'Local setup', content: setup });
  }
  const people = element('section', 'settings-section');
  people.innerHTML = `<h2>Workspace access</h2><p class="page-subtitle">Manage membership and resource access from Users. Switch workspaces from your account menu.</p><a class="ui-button" href="${workspaceId ? workspaceUrl('/?view=people') : '/?view=access-preview'}">${workspaceId ? 'Open Users' : 'Explore access preview'}</a>`;
  sections.push({ title: 'Workspace', content: people });
  installTabs(content, sections);
  const theme = app.querySelector<HTMLSelectElement>('#theme')!;
  theme.value = document.documentElement.dataset.theme ?? 'system';
  theme.onchange = () => {
    if (theme.value === 'system') delete document.documentElement.dataset.theme;
    else document.documentElement.dataset.theme = theme.value;
    try { localStorage.setItem('ayati.theme', theme.value); } catch { /* Preference still works for this page. */ }
  };
  if (!canManageResources()) return;
  installSetup();
  if (workspaceId) {
    connections.append(element('h2', '', 'Business connections'), element('p', 'page-subtitle', 'Workspace connections are not set up yet. Local account connections are kept separate.'));
  } else installConnections(connections);
}

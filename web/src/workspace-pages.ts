import { installSetup } from './setup';
import { installConnections } from './connections';
import { installApps } from './apps';

export function installWorkspacePage(page: 'apps' | 'settings'): void {
  document.title = `${page === 'apps' ? 'Apps' : 'Settings'} · Ayati`;
  const app = document.querySelector('#app')!;
  if (page === 'apps') {
    installApps();
    return;
  }
  app.innerHTML = `<main class="workspace-page"><h1>Settings</h1><p class="page-subtitle">Your workspace, appearance and setup.</p>
    <section class="settings-section"><h2>Appearance</h2><label class="settings-row">Theme<select id="theme"><option value="system">Use system setting</option><option value="light">Light</option><option value="dark">Dark</option></select></label></section>
    <section class="settings-section"><h2>Local setup</h2><p id="setup-status" role="status">Checking connection…</p><div id="setup-facts"></div><p class="page-subtitle">Provider credentials are configured locally. Restart Ayati after changing configuration.</p></section>
    <section class="settings-section" id="business-connections"></section>
    <section class="settings-section"><h2>People and usage</h2><p class="page-subtitle">This workspace currently has one local owner. Explore signup, workspaces and employee invitations with separate browser-only demo data.</p><a href="/?view=access-preview">Try the workspace access preview</a><p class="page-subtitle">Real authentication, staff access and spending controls are not connected yet.</p></section></main>`;
  const theme = document.querySelector<HTMLSelectElement>('#theme')!;
  theme.value = document.documentElement.dataset.theme ?? 'system';
  theme.onchange = () => {
    if (theme.value === 'system') delete document.documentElement.dataset.theme;
    else document.documentElement.dataset.theme = theme.value;
    try { localStorage.setItem('ayati.theme', theme.value); } catch { /* Preference still works for this page. */ }
  };
  installSetup();
  installConnections(document.querySelector('#business-connections')!);
}

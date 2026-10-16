import { installSetup } from './setup';

export function installWorkspacePage(page: 'apps' | 'settings'): void {
  document.title = `${page === 'apps' ? 'Apps' : 'Settings'} · Ayati`;
  const app = document.querySelector('#app')!;
  if (page === 'apps') {
    app.innerHTML = `<main class="workspace-page"><h1>Apps</h1><p class="page-subtitle">The tools that keep your business moving.</p>
      <section class="empty-surface"><span class="empty-mark" aria-hidden="true">▦</span><h2>Your apps will live here</h2>
      <p>Installed business apps and app creation are still being built. Your agents are available now.</p><a class="files-button" href="/?agent=local-agent">Open Agents</a></section></main>`;
    return;
  }
  app.innerHTML = `<main class="workspace-page"><h1>Settings</h1><p class="page-subtitle">Your workspace, appearance and setup.</p>
    <section class="settings-section"><h2>Appearance</h2><label class="settings-row">Theme<select id="theme"><option value="system">Use system setting</option><option value="light">Light</option><option value="dark">Dark</option></select></label></section>
    <section class="settings-section"><h2>Local setup</h2><p id="setup-status" role="status">Checking connection…</p><div id="setup-facts"></div><p class="page-subtitle">Provider credentials are configured locally. Restart Ayati after changing configuration.</p></section>
    <section class="settings-section"><h2>Connections, people and usage</h2><p class="page-subtitle">Business account connections, staff access and spending controls are not available yet. This workspace currently has one local owner.</p></section></main>`;
  const theme = document.querySelector<HTMLSelectElement>('#theme')!;
  theme.value = document.documentElement.dataset.theme ?? 'system';
  theme.onchange = () => {
    if (theme.value === 'system') delete document.documentElement.dataset.theme;
    else document.documentElement.dataset.theme = theme.value;
    try { localStorage.setItem('ayati.theme', theme.value); } catch { /* Preference still works for this page. */ }
  };
  installSetup();
}

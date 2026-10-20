import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { flushSync } from 'react-dom';
import { enterWorkspace, currentWorkspace, workspaceId } from './workspace-context';
import { WorkspaceShell, type WorkspacePage } from './react/workspace-shell';
import { AccountPage } from './react/account';
import { PeoplePage } from './react/people';
import './style.css';
import './workspace.css';
import './workspace-menus.css';
import './brand.css';
import './access-preview.css';
import './app-ui.css';
import './agent-manager.css';

const root = createRoot(document.querySelector<HTMLDivElement>('#root')!);

/** This subtree has one DOM owner. Existing feature installers manage its contents until converted. */
function LegacySurface() { return <div id="app" />; }

async function installLegacy(page: WorkspacePage): Promise<void> {
  if (page === 'agents') await import('./agent-manager').then(module => module.installAgentManager());
  else if (page === 'chat' && workspaceId && !new URLSearchParams(location.search).has('agent')) {
    await import('./shared-chat').then(module => module.installSharedChat());
  } else if (page === 'home') await import('./home').then(module => module.installHome());
  else if (page === 'chat') await import('./chat');
  else if (page === 'apps' || page === 'settings') await import('./workspace-pages').then(module => module.installWorkspacePage(page));
}

async function start(): Promise<void> {
  try {
    const theme = localStorage.getItem('ayati.theme');
    if (theme === 'light' || theme === 'dark') document.documentElement.dataset.theme = theme;
  } catch { /* System preference is the fallback. */ }
  flushSync(() => root.render(<main className="workspace-loading" role="status">Opening Relay…</main>));
  if (!await enterWorkspace()) return;
  const params = new URLSearchParams(location.search);
  const requested = params.get('view');
  if (!workspaceId && requested === 'account') {
    root.render(<StrictMode><AccountPage /></StrictMode>); return;
  }
  if (requested === 'access-preview') {
    flushSync(() => root.render(<LegacySurface />));
    await import('./access-preview').then(module => module.installAccessPreview()); return;
  }
  const page: WorkspacePage = requested === 'people' || requested === 'users' ? 'people'
    : requested === 'account' ? 'account' : params.has('agent') || requested === 'chat' ? 'chat'
    : requested === 'agents' || requested === 'apps' || requested === 'settings' ? requested : 'home';
  if (page === 'chat' && params.has('agent') && workspaceId && currentWorkspace()?.role !== 'Owner') {
    location.replace(`/w/${workspaceId}/?view=chat`); return;
  }
  document.documentElement.dataset.page = page;
  if (params.get('guide') === '1' && window.self !== window.top) document.documentElement.classList.add('guide-mode');
  if (page === 'people') document.title = 'Users · Relay';
  flushSync(() => root.render(<StrictMode><WorkspaceShell page={page}>
    {page === 'people' ? <PeoplePage /> : page === 'account' ? <AccountPage embedded /> : <LegacySurface />}
  </WorkspaceShell></StrictMode>));
  // Feature and workspace navigation deliberately loads a new document. Legacy streams/timers
  // live for that document only; do not add in-place routing until each installer has teardown.
  if (page !== 'people' && page !== 'account') await installLegacy(page);
}

void start().catch(error => {
  root.render(<div id="app" role="alert">Unable to open workspace: {error instanceof Error ? error.message : 'Please reload to retry.'}</div>);
});

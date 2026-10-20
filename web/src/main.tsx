import { WorkspaceApplication } from './react/workspace-application';
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import { flushSync } from 'react-dom';
import { enterWorkspace, workspaceId } from './workspace-context';
import { AccountPage } from './react/account';
import './style.css';
import './workspace.css';
import './workspace-menus.css';
import './brand.css';
import './access-preview.css';
import './app-ui.css';
import './agent-manager.css';

const root = createRoot(document.querySelector<HTMLDivElement>('#root')!);

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
    flushSync(() => root.render(<div id="app" />));
    await import('./access-preview').then(module => module.installAccessPreview()); return;
  }
  if (params.get('guide') === '1' && window.self !== window.top) document.documentElement.classList.add('guide-mode');
  root.render(<StrictMode><WorkspaceApplication /></StrictMode>);
}

void start().catch(error => {
  root.render(<div id="app" role="alert">Unable to open workspace: {error instanceof Error ? error.message : 'Please reload to retry.'}</div>);
});

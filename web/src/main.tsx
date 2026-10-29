import { SimulationPage } from './react/simulation';
import './react/company.css';
import './react/agents/control.css';
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
import './account.css';
import './app-ui.css';
import './agent-manager.css';

const root = createRoot(document.querySelector<HTMLDivElement>('#root')!);

async function start(): Promise<void> {
  try {
    const theme = localStorage.getItem('ayati.theme');
    if (theme === 'light' || theme === 'dark') document.documentElement.dataset.theme = theme;
  } catch { /* System preference is the fallback. */ }
  flushSync(() => root.render(<main className="workspace-loading" role="status">Opening Cuckoo…</main>));
  if (new URLSearchParams(location.search).get("view") === "simulation") { root.render(<StrictMode><SimulationPage /></StrictMode>); return; }
  if (!await enterWorkspace()) return;
  // Outside a workspace the account page is the sign-in and workspace chooser.
  if (!workspaceId) { root.render(<StrictMode><AccountPage /></StrictMode>); return; }
  root.render(<StrictMode><WorkspaceApplication /></StrictMode>);
}

void start().catch(error => {
  root.render(<div id="app" role="alert">Unable to open workspace: {error instanceof Error ? error.message : 'Please reload to retry.'}</div>);
});

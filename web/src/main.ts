import { enterWorkspace, currentWorkspace, workspaceId } from './workspace-context';
import './style.css';
import './workspace.css';
import { installWorkspace, type WorkspacePage } from './workspace';

async function start(): Promise<void> {
  if (!await enterWorkspace()) return;
  const params = new URLSearchParams(location.search);
  const requested = params.get('view');
  if (!workspaceId && requested === 'account') {
    await import('./account').then(({ installAccount }) => installAccount());
  } else if (requested === 'access-preview') {
    await import('./access-preview').then(({ installAccessPreview }) => installAccessPreview());
  } else {
    const page: WorkspacePage = requested === 'people' || requested === 'account' ? requested : params.has('agent') ? 'agents'
      : requested === 'agents' || requested === 'apps' || requested === 'settings' ? requested : 'home';
    installWorkspace(page);
    if (page === 'people' || page === 'account') {
      await import('./account').then(({ installAccount }) => installAccount(page));
    } else if (workspaceId && currentWorkspace()?.role !== 'Owner' && page === 'agents') {
      const app = document.querySelector<HTMLDivElement>('#app')!;
      const title = document.createElement('h1'); title.textContent = 'Chat & agents';
      const detail = document.createElement('p'); detail.textContent = 'You’re in the right workspace. Business resources and agent actions are currently available to the workspace owner. You can view your team in People.';
      app.className = 'workspace-page'; app.append(title, detail);
    } else if (page === 'home') {
      await import('./home').then(({ installHome }) => installHome());
    } else if (page === 'agents') {
      await import('./chat');
    } else {
      await import('./workspace-pages').then(({ installWorkspacePage }) => installWorkspacePage(page));
    }
  }

}
void start().catch(error => { const app = document.querySelector<HTMLDivElement>("#app")!; app.textContent = `Unable to open workspace: ${error.message}. Reload to retry.`; });

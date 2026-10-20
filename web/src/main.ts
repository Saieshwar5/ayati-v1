import { enterWorkspace, currentWorkspace, workspaceId } from './workspace-context';
import './style.css';
import './workspace.css';
import './app-ui.css';
import './agent-manager.css';
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
    const page: WorkspacePage = requested === 'people' || requested === 'users' ? 'people'
      : requested === 'account' ? 'account' : params.has('agent') || requested === 'chat' ? 'chat'
      : requested === 'agents' || requested === 'apps' || requested === 'settings' ? requested : 'home';
    installWorkspace(page);
    if (page === 'people') {
      await import('./people').then(({ installPeople }) => installPeople());
    } else if (page === 'account') {
      await import('./account').then(({ installAccount }) => installAccount('account'));
    } else if (page === 'agents') {
      await import('./agent-manager').then(({ installAgentManager }) => installAgentManager());
    } else if (page === 'chat' && workspaceId && !params.has('agent')) {
      await import('./shared-chat').then(({ installSharedChat }) => installSharedChat());
    } else if (page === 'chat' && workspaceId && currentWorkspace()?.role !== 'Owner') {
      location.replace(`/w/${workspaceId}/?view=chat`);
    } else if (page === 'home') {
      await import('./home').then(({ installHome }) => installHome());
    } else if (page === 'chat') {
      await import('./chat');
    } else {
      await import('./workspace-pages').then(({ installWorkspacePage }) => installWorkspacePage(page));
    }
  }

}
void start().catch(error => { const app = document.querySelector<HTMLDivElement>("#app")!; app.textContent = `Unable to open workspace: ${error.message}. Reload to retry.`; });

import './style.css';
import './workspace.css';
import { installWorkspace, type WorkspacePage } from './workspace';

const params = new URLSearchParams(location.search);
const requested = params.get('view');
if (requested === 'account') {
  void import('./account').then(({ installAccount }) => installAccount());
} else if (requested === 'access-preview') {
  void import('./access-preview').then(({ installAccessPreview }) => installAccessPreview());
} else {
  const page: WorkspacePage = params.has('agent') ? 'agents'
    : requested === 'agents' || requested === 'apps' || requested === 'settings' ? requested : 'home';
  installWorkspace(page);
  if (page === 'home') {
    void import('./home').then(({ installHome }) => installHome());
  } else if (page === 'agents') {
    void import('./chat');
  } else {
    void import('./workspace-pages').then(({ installWorkspacePage }) => installWorkspacePage(page));
  }
}

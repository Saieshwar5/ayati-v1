import { startLive } from './chat/live';
import { lazy, Suspense, useEffect, useSyncExternalStore } from 'react';
import { installNavigation, subscribeNavigation, workspacePage } from '../navigation';
import { WorkspaceShell } from './workspace-shell';
import { SettingsPage } from './settings/settings-page';
import { AccountPage } from './account';
import { HomePage } from './home-page';
import './agents/agents.css';
import './agents/context-preview.css';
import { AgentsPage } from './agents/directory';
import { WorkspaceChat } from './chat/workspace-chat';
import { LibraryUploads } from './library/uploads';
import { LibraryBoundary } from './library/boundary';
import './library/library.css';
const LibraryPage = lazy(() => import('./library/library-page').then(module => ({ default: module.LibraryPage })));

export function WorkspaceApplication() {
  const href = useSyncExternalStore(subscribeNavigation, () => window.location.href);
  const page = workspacePage(new URL(href));
  useEffect(installNavigation, []);
  useEffect(startLive, []);
  useEffect(() => {
    document.documentElement.dataset.page = page;
    const title = page === 'account' ? 'My account' : page[0].toUpperCase() + page.slice(1);
    document.title = `${title} · Cuckoo`;
  }, [page, href]);
  return <LibraryUploads><WorkspaceShell page={page}>
    {page === 'agents' ? <AgentsPage /> : page === 'settings' ? <SettingsPage href={href} /> : page === 'chat' ? <WorkspaceChat href={href} />
      : page === 'account' ? <AccountPage embedded /> : page === 'library' ? <LibraryBoundary><Suspense fallback={<p role="status">Opening Library…</p>}><LibraryPage href={href} /></Suspense></LibraryBoundary>
      : <HomePage />}
  </WorkspaceShell></LibraryUploads>;
}

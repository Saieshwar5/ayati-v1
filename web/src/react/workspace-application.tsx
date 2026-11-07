import { startLive } from './chat/live';
import { startDesktopAlerts } from './chat/notifications';
import { startUnreadCounts } from './chat/unread-counts';
import { useUnreadTitle } from './chat/unread-badge';
import { lazy, Suspense, useEffect, useSyncExternalStore } from 'react';
import { installNavigation, subscribeNavigation, workspacePage } from '../navigation';
import { WorkspaceShell, type WorkspacePage } from './workspace-shell';
import { SettingsPage } from './settings/settings-page';
import { HomePage } from './home-page';
import './agents/agents.css';
import './agents/context-preview.css';
import { AgentsPage } from './agents/directory';
import { WorkspaceChat } from './chat/workspace-chat';
import { LibraryUploads } from './library/uploads';
import { LibraryBoundary } from './library/boundary';
import './library/library.css';
const ActivityPage = lazy(() => import('./activity/activity-page').then(module => ({ default: module.ActivityPage })));
const AppsPage = lazy(() => import('./apps/apps-page').then(module => ({ default: module.AppsPage })));
const LibraryPage = lazy(() => import('./library/library-page').then(module => ({ default: module.LibraryPage })));

export function WorkspaceApplication() {
  const href = useSyncExternalStore(subscribeNavigation, () => window.location.href);
  const page = workspacePage(new URL(href));
  useEffect(installNavigation, []);
  useEffect(startLive, []);
  // One unread reader for the rail, the tab title, Chat and alerts (D245).
  useEffect(() => startUnreadCounts(), []);
  useEffect(startDesktopAlerts, []);
  useEffect(() => { document.documentElement.dataset.page = page; }, [page]);
  return <LibraryUploads><WorkspaceTitle page={page} /><WorkspaceShell page={page}>
    {page === 'agents' ? <AgentsPage /> : page === 'settings' ? <SettingsPage href={href} /> : page === 'chat' ? <WorkspaceChat href={href} />
      : page === 'activity' ? <Suspense fallback={<p role="status">Opening Activity…</p>}><ActivityPage key={href} href={href} /></Suspense>
      : page === 'apps' ? <Suspense fallback={<p role="status">Opening Apps…</p>}><AppsPage key={href} href={href} /></Suspense>
      : page === 'library' ? <LibraryBoundary><Suspense fallback={<p role="status">Opening Library…</p>}><LibraryPage href={href} /></Suspense></LibraryBoundary>
      : <HomePage />}
  </WorkspaceShell></LibraryUploads>;
}

/** The tab title names the page, after the rail's Chat count when something is for you. */
function WorkspaceTitle({ page }: { page: WorkspacePage }) {
  useUnreadTitle(`${page[0].toUpperCase() + page.slice(1)} · Cuckoo`);
  return null;
}

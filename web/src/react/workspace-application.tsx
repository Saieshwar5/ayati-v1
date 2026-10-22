import { startLive } from './chat/live';
import { disposeViewLifetime } from '../view-lifetime';
import { useEffect, useSyncExternalStore } from 'react';
import { installNavigation, subscribeNavigation, workspacePage } from '../navigation';
import { currentWorkspace, workspaceId } from '../workspace-context';
import { WorkspaceShell } from './workspace-shell';
import { SettingsPage } from './settings/settings-page';
import { AccountPage } from './account';
import { PrivateChat } from './private-chat';
import { LegacyView } from './legacy-view';
import './agents/agents.css';
import { AgentsPage } from './agents/directory';
import { WorkspaceChat } from './chat/workspace-chat';

export function WorkspaceApplication() {
  const href = useSyncExternalStore(subscribeNavigation, () => window.location.href);
  const url = new URL(href);
  const page = workspacePage(url);
  const privateChat = page === 'chat' && (url.searchParams.has('agent') || !workspaceId);
  useEffect(installNavigation, []);
  useEffect(startLive, []);
  const featureKey = page === 'chat' ? 'chat' : href;
  useEffect(() => () => disposeViewLifetime(), [featureKey]);
  useEffect(() => {
    document.documentElement.dataset.page = page;
    const title = page === 'account' ? 'My account' : page[0].toUpperCase() + page.slice(1);
    document.title = `${title} · Cuckoo`;
  }, [page, href]);
  const denied = privateChat && workspaceId && currentWorkspace()?.role !== 'Owner';
  return <WorkspaceShell page={page}>
    {page === 'agents' && workspaceId && !url.searchParams.has('legacy') ? <AgentsPage /> : page === 'settings' ? <SettingsPage href={href} /> : page === 'chat' && workspaceId ? <WorkspaceChat href={href} /> : denied ? <p role="alert">Private agent chats require workspace owner access.</p> : page === 'account' ? <AccountPage embedded />
      : privateChat ? <PrivateChat href={href} /> : <LegacyView key={featureKey} page={page} href={href} />}
  </WorkspaceShell>;
}

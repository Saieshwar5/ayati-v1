import { disposeViewLifetime } from '../view-lifetime';
import { useEffect, useSyncExternalStore } from 'react';
import { installNavigation, subscribeNavigation, workspacePage } from '../navigation';
import { currentWorkspace, workspaceId } from '../workspace-context';
import { WorkspaceShell } from './workspace-shell';
import { PeoplePage } from './people';
import { AccountPage } from './account';
import { PrivateChat } from './private-chat';
import { LegacyView } from './legacy-view';

export function WorkspaceApplication() {
  const href = useSyncExternalStore(subscribeNavigation, () => window.location.href);
  const url = new URL(href);
  const page = workspacePage(url);
  const privateChat = page === 'chat' && (url.searchParams.has('agent') || !workspaceId);
  useEffect(installNavigation, []);
  const featureKey = page === 'chat' && !privateChat ? 'shared-chat' : href;
  useEffect(() => () => disposeViewLifetime(), [featureKey]);
  useEffect(() => {
    document.documentElement.dataset.page = page;
    const title = page === 'people' ? 'Users' : page === 'account' ? 'My account' : page[0].toUpperCase() + page.slice(1);
    document.title = `${title} · Relay`;
  }, [page]);
  const denied = privateChat && workspaceId && currentWorkspace()?.role !== 'Owner';
  return <WorkspaceShell page={page}>
    {denied ? <p role="alert">Private agent chats require workspace owner access.</p> : page === 'people' ? <PeoplePage /> : page === 'account' ? <AccountPage embedded />
      : privateChat ? <PrivateChat href={href} /> : <LegacyView key={featureKey} page={page} href={href} />}
  </WorkspaceShell>;
}

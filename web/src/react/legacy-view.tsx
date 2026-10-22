import { HomeNotifications, type HomeNotificationsState, type HomeNotice } from './home-notifications';
import { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { workspaceId } from '../workspace-context';
import { WorkRecords } from './work-records';
import { HomeToolbar } from './home-toolbar';
import { ViewLifetime, setViewLifetime } from '../view-lifetime';
import type { Profile } from '../agents';
import type { WorkspacePage } from './workspace-shell';

type Controller = { profile?: (profile: Profile) => void; navigate?: (url: URL) => void };

async function install(page: WorkspacePage, view: ViewLifetime, onNotifications: (data: HomeNotificationsState) => void): Promise<Controller | void> {
  if (page === 'agents') {
    const module = await import('../agent-manager');
    if (view.active) { setViewLifetime(view); await module.installAgentManager(); }
  } else if (page === 'home') {
    const module = await import('../home');
    if (view.active) { setViewLifetime(view); module.installHome(onNotifications); }
  } else if (page === 'chat') {
    const module = await import('../chat');
    if (view.active) { setViewLifetime(view); return { profile: module.installChat() }; }
  } else if (page === 'apps') {
    const module = await import('../apps');
    if (view.active) { setViewLifetime(view); module.installApps(); }
  }
}

/** React owns the host; the feature owns its subtree and must dispose every effect. */
export function LegacyView({ page, href, profile }: { page: WorkspacePage; href: string; profile?: Profile }) {
  const [notifications, setNotifications] = useState<HomeNotificationsState | null>(null);
  const [notificationHost, setNotificationHost] = useState<HTMLElement | null>(null);
  const [ready, setReady] = useState(false);
  const [workHost, setWorkHost] = useState<HTMLElement | null>(null);
  const [homeActions, setHomeActions] = useState<HTMLElement | null>(null);
  const controller = useRef<Controller | void>(undefined);
  const latestHref = useRef(href); latestHref.current = href;
  const mountKey = href;
  useEffect(() => { controller.current?.navigate?.(new URL(href)); }, [href]);
  const latestProfile = useRef(profile); latestProfile.current = profile;
  useEffect(() => { if (profile) controller.current?.profile?.(profile); }, [profile]);
  const host = useRef<HTMLDivElement>(null);
  useEffect(() => {
    setReady(false);
    setWorkHost(null);
    setNotifications(null); setNotificationHost(null);
    setHomeActions(null);
    // Each effect owns a separate subtree, including Strict Mode remounts. Late
    // catch/finally handlers can only touch their detached original nodes.
    const node = document.createElement('div'); node.id = 'app';
    host.current!.append(node);
    const view = new ViewLifetime(node, new URL(href));
    setViewLifetime(view);
    const status = document.createElement('p'); status.setAttribute('role', 'status');
    status.className = 'feature-loading'; status.textContent = 'Opening…'; node.append(status);
    void install(page, view, data => { if (view.active) setNotifications(data); }).then(consumer => {
      if (!view.active) return;
      status.remove(); controller.current = consumer;
      setReady(true);
      if (page === 'home') {
        setHomeActions(node.querySelector('[data-home-actions]'));
        setWorkHost(node.querySelector('[data-work-records]'));
        setNotificationHost(node.querySelector('[data-home-notifications]'));
      }
      consumer?.navigate?.(new URL(latestHref.current));
      if (latestProfile.current) consumer?.profile?.(latestProfile.current);
    }).catch(error => {
      if (!view.active) return;
      node.replaceChildren();
      const message = document.createElement('p'); message.setAttribute('role', 'alert');
      message.textContent = `Unable to open this section: ${error instanceof Error ? error.message : 'Retry shortly.'}`;
      const reload = document.createElement('button'); reload.type = 'button';
      reload.className = 'ui-button'; reload.textContent = 'Reload application';
      reload.onclick = () => location.reload();
      node.append(message, reload);
    });
    return () => { controller.current = undefined; view.dispose(); node.remove(); };
  }, [page, mountKey]);
  const trigger = (selector: string) => host.current?.querySelector<HTMLButtonElement>(selector)?.click();
  const openNotifications = (category: HomeNotice['category']) => {
    trigger('#home-decisions');
    trigger(`.inbox-filters [data-category="${category}"]`);
  };
  if (page !== 'home') return <div className="feature-host" ref={host} />;
  return <div className="feature-host home-feature">
    {workspaceId && workHost && createPortal(<WorkRecords />, workHost)}
    {notificationHost && createPortal(<HomeNotifications data={notifications} open={openNotifications} />, notificationHost)}
    {homeActions && createPortal(<HomeToolbar ready={ready}
      arrange={() => trigger('[data-arrange]')} add={() => trigger('[data-request-panel]')} />, homeActions)}
    <div className="feature-host" ref={host} />
  </div>;
}

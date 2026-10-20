import { useEffect, useRef } from 'react';
import { ViewLifetime, setViewLifetime } from '../view-lifetime';
import { workspaceId } from '../workspace-context';
import type { Profile } from '../agents';
import type { WorkspacePage } from './workspace-shell';

type Controller = { profile?: (profile: Profile) => void; navigate?: (url: URL) => void };

async function install(page: WorkspacePage, view: ViewLifetime): Promise<Controller | void> {
  const url = view.url;
  if (page === 'agents') {
    const module = await import('../agent-manager');
    if (view.active) { setViewLifetime(view); await module.installAgentManager(); }
  } else if (page === 'chat' && workspaceId && !url.searchParams.has('agent')) {
    const module = await import('../shared-chat');
    if (view.active) { setViewLifetime(view); return { navigate: await module.installSharedChat() }; }
  } else if (page === 'home') {
    const module = await import('../home');
    if (view.active) { setViewLifetime(view); module.installHome(); }
  } else if (page === 'chat') {
    const module = await import('../chat');
    if (view.active) { setViewLifetime(view); return { profile: module.installChat() }; }
  } else if (page === 'apps' || page === 'settings') {
    const module = await import('../workspace-pages');
    if (view.active) { setViewLifetime(view); module.installWorkspacePage(page); }
  }
}

/** React owns the host; the feature owns its subtree and must dispose every effect. */
export function LegacyView({ page, href, profile }: { page: WorkspacePage; href: string; profile?: Profile }) {
  const controller = useRef<Controller | void>(undefined);
  const latestHref = useRef(href); latestHref.current = href;
  const mountKey = page === 'chat' && workspaceId && !new URL(href).searchParams.has('agent') ? 'shared-chat' : href;
  useEffect(() => { controller.current?.navigate?.(new URL(href)); }, [href]);
  const latestProfile = useRef(profile); latestProfile.current = profile;
  useEffect(() => { if (profile) controller.current?.profile?.(profile); }, [profile]);
  const host = useRef<HTMLDivElement>(null);
  useEffect(() => {
    // Each effect owns a separate subtree, including Strict Mode remounts. Late
    // catch/finally handlers can only touch their detached original nodes.
    const node = document.createElement('div'); node.id = 'app';
    host.current!.append(node);
    const view = new ViewLifetime(node, new URL(href));
    setViewLifetime(view);
    const status = document.createElement('p'); status.setAttribute('role', 'status');
    status.className = 'feature-loading'; status.textContent = 'Opening…'; node.append(status);
    void install(page, view).then(consumer => {
      if (!view.active) return;
      status.remove(); controller.current = consumer;
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
  return <div className="feature-host" ref={host} />;
}

import type { WorkspacePage } from './react/workspace-shell';

const eventName = 'relay:navigate';
export function workspacePage(url: URL): WorkspacePage {
  const requested = url.searchParams.get('view');
  if (requested === 'people' || requested === 'users') return 'settings';
  if (requested === 'company' || requested === 'feedback') return 'settings';
  if (requested === 'account') return 'account';
  if (requested === 'chat') return 'chat';
  return requested === 'agents' || requested === 'library' || requested === 'settings' ? requested : 'home';
}
export function localDestination(href: string, current = new URL(window.location.href)): URL | undefined {
  const url = new URL(href, current);
  // Account/workspace changes must discard the entire old identity and its state.
  if (url.origin !== current.origin || url.pathname !== current.pathname) return;
  if (!/^\/w\/[0-9a-f-]{36}\/$/.test(current.pathname) && current.pathname !== '/' && current.pathname !== '/index.html') return;
  if (url.searchParams.get('view') === 'access-preview') return;
  if (url.search === current.search && url.hash !== current.hash) return;
  return url;
}
export function navigate(href: string): void {
  const destination = localDestination(href);
  if (!destination) { location.assign(href); return; }
  if (destination.href === window.location.href) return;
  window.history.pushState(null, '', destination);
  window.dispatchEvent(new window.Event(eventName));
}
export function subscribeNavigation(changed: () => void): () => void {
  const pop = () => changed();
  window.addEventListener(eventName, changed);
  window.addEventListener('popstate', pop);
  return () => { window.removeEventListener(eventName, changed); window.removeEventListener('popstate', pop); };
}
export function installNavigation(): () => void {
  const clicked = (event: MouseEvent) => {
    if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    const anchor = event.target instanceof window.Element ? event.target.closest<HTMLAnchorElement>('a[href]') : null;
    if (!anchor || anchor.hasAttribute('download') || (anchor.target && anchor.target !== '_self') || !localDestination(anchor.href)) return;
    event.preventDefault();
    // Popovers live outside the shell; dismiss them before navigating away.
    document.querySelectorAll<HTMLElement>('[popover]').forEach(node => { if (typeof node.hidePopover === 'function') node.hidePopover(); });
    navigate(anchor.href);
  };
  document.addEventListener('click', clicked);
  return () => document.removeEventListener('click', clicked);
}

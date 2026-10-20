import { api as requestApi, apiUrl as requestUrl } from './types';
import { identityRequest as requestIdentity } from './workspace-context';

declare global {
  interface WindowEventMap { 'business-clock-changed': Event }
}

/** Owns one feature's requests, portals and subscriptions, never runtime authority. */
export class ViewLifetime {
  readonly abort = new AbortController();
  readonly agent: string;
  private cleanups: (() => void)[] = [];
  private portals = new Set<HTMLElement>();
  active = true;
  constructor(readonly root: HTMLElement, readonly url = new URL(window.location.href)) {
    this.agent = url.searchParams.get('agent') ?? 'local-agent';
  }
  check = (): void => {
    if (!this.active) throw new DOMException('View closed', 'AbortError');
  };
  api = async <T>(path: string, options?: RequestInit): Promise<T> => {
    this.check();
    const signal = options?.signal ? AbortSignal.any([this.abort.signal, options.signal]) : this.abort.signal;
    const data = await requestApi<T>(path, { ...options, signal }, this.agent);
    this.check();
    return data;
  };
  post = <T>(path: string, body?: unknown): Promise<T> => this.api<T>(path, {
    method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body ?? {}),
  });
  apiUrl = (path: string): string => requestUrl(path, this.agent);
  identityRequest = async <T>(path: string, body?: unknown): Promise<T> => {
    this.check();
    const data = await requestIdentity<T>(path, body, this.abort.signal);
    this.check();
    return data;
  };
  querySelector<T extends Element = Element>(selector: string): T | null {
    if (this.root.matches(selector)) return this.root as unknown as T;
    return this.root.querySelector<T>(selector)
      ?? [...this.portals].map(node => node.matches(selector) ? node as unknown as T : node.querySelector<T>(selector)).find(Boolean) ?? null;
  }
  querySelectorAll<T extends Element = Element>(selector: string): T[] {
    return [...this.root.querySelectorAll<T>(selector), ...[...this.portals].flatMap(node => [...node.querySelectorAll<T>(selector)])];
  }
  cleanup = (action: () => void): void => {
    if (this.active) this.cleanups.push(action); else action();
  };
  append = (...nodes: HTMLElement[]): void => {
    if (!this.active) return;
    for (const node of nodes) {
      this.portals.add(node); document.body.append(node);
      this.cleanup(() => { if (node.tagName === 'DIALOG' && (node as HTMLDialogElement).open) (node as HTMLDialogElement).close(); node.remove(); this.portals.delete(node); });
    }
  };
  interval = (action: () => void, ms: number): ReturnType<typeof setInterval> => {
    const timer = window.setInterval(() => { if (this.active) action(); }, ms);
    this.cleanup(() => window.clearInterval(timer)); return timer;
  };
  timeout = (action: () => void, ms: number): ReturnType<typeof setTimeout> => {
    const timer = window.setTimeout(() => { if (this.active) action(); }, ms);
    this.cleanup(() => window.clearTimeout(timer)); return timer;
  };
  onWindow<K extends keyof WindowEventMap>(event: K, action: (event: WindowEventMap[K]) => void): void {
    const listener = (e: WindowEventMap[K]) => { if (this.active) action(e); };
    window.addEventListener(event, listener); this.cleanup(() => window.removeEventListener(event, listener));
  }
  onDocument<K extends keyof DocumentEventMap>(event: K, action: (event: DocumentEventMap[K]) => void): void {
    const listener = (e: DocumentEventMap[K]) => { if (this.active) action(e); };
    document.addEventListener(event, listener); this.cleanup(() => document.removeEventListener(event, listener));
  }
  onMedia(media: MediaQueryList, action: (event: MediaQueryListEvent) => void): void {
    media.addEventListener('change', action); this.cleanup(() => media.removeEventListener('change', action));
  }
  dispose(): void {
    if (!this.active) return;
    this.active = false; this.abort.abort();
    for (const action of this.cleanups.reverse()) action();
    this.cleanups = [];
  }
}

let installed: ViewLifetime | undefined;
export function setViewLifetime(view: ViewLifetime): void { installed = view; }
export function currentView(): ViewLifetime {
  // Standalone feature tests and the access preview can initialize without the router.
  const root = document.querySelector<HTMLElement>('#app');
  if (!installed || installed.root !== root) {
    if (!root) throw new Error('Feature surface is unavailable');
    installed = new ViewLifetime(root);
  }
  return installed;
}

export function disposeViewLifetime(): void { installed?.dispose(); }

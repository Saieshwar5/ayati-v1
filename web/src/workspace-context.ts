export type Membership = { id: string; name: string; role: string };
type Session = { simulation?: boolean; user: { id: string; email: string; display_name?: string | null } | null; workspaces?: Membership[] };
const match = (typeof location === 'undefined' ? '' : location.pathname).match(/^\/w\/([0-9a-f-]{36})\//);
export const workspaceId = match?.[1] ?? '';
export let workspaceSession: Session | undefined;
export function workspaceUrl(path = '/'): string { return workspaceId ? `/w/${workspaceId}${path}` : path; }
export function storageScope(key: string): string {
  return workspaceId ? `${key}.${workspaceSession?.user?.id ?? 'signed-out'}.${workspaceId}` : key;
}
export function currentWorkspace(): Membership | undefined { return workspaceSession?.workspaces?.find(w => w.id === workspaceId); }
export async function identityRequest<T>(path: string, body?: unknown, signal?: AbortSignal): Promise<T> {
  const response = await fetch(`/api/identity${path}`, { method: body === undefined ? 'GET' : 'POST', signal,
    headers: body === undefined ? {} : { 'Content-Type': 'application/json', 'X-Ayati-Request': 'identity' },
    body: body === undefined ? undefined : JSON.stringify(body) });
  if (path === '/session' && response.status === 401) return { user: null } as T;
  if (path === '/profile' && response.status === 401) location.replace('/?view=account');
  const data = await response.json();
  if (!response.ok) throw Object.assign(new Error(data.error ?? 'Account request failed.'), { status: response.status, retryAfter: response.headers?.get('Retry-After') });
  return data as T;
}
export async function enterWorkspace(): Promise<boolean> {
  if (!workspaceId) return true;
  workspaceSession = await identityRequest<Session>('/session');
  if (!workspaceSession.user || !currentWorkspace()) { location.replace('/?view=account&choose=1'); return false; }
  if (workspaceSession.user.display_name === null) { location.replace('/?view=account&choose=1'); return false; }
  try { localStorage.setItem(`relay.last-workspace.${workspaceSession.user.id}`, workspaceId); } catch { /* Optional preference. */ }
  // Workspace/account switches reload the document; feature switches use view lifetimes.
  let checking = false;
  setInterval(async () => {
    if (checking) return; checking = true;
    try {
      const next = await identityRequest<Session>('/session');
      if (next.user?.id !== workspaceSession?.user?.id || !next.workspaces?.some(w => w.id === workspaceId && w.role === currentWorkspace()?.role)) {
        location.replace('/?view=account&choose=1');
      }
    } catch { /* APIs fail closed while the provider is unavailable. */ }
    finally { checking = false; }
  }, 15000);
  return true;
}

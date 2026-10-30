import type { AccountUser } from '../display-name-form';

export type Workspace = { id: string; name: string; role: string; people?: number; agents?: number };
export type Invitation = { id: string; name: string; role: string; expires?: number; inviter?: string | null };
export type Session = {
  user: AccountUser | null; workspaces?: Workspace[]; invitations?: Invitation[];
  platform_admin?: boolean; simulation?: boolean;
};
export type WorkspaceCode = {
  id: string; note: string; created_at: number; expires_at: number; used_at?: number | null;
  status: 'unused' | 'used' | 'expired' | 'revoked';
  used_by?: { name: string; email: string } | null; workspace?: { id: string; name: string } | null;
};

/** Thrown when the session ended, so the page can return to sign-in. */
export class SignedOut extends Error {}

export async function accessRequest<T>(path: string, body?: unknown): Promise<T> {
  const response = await fetch(`/api/identity${path}`, {
    method: body === undefined ? 'GET' : 'POST', credentials: 'same-origin',
    headers: body === undefined ? {} : { 'Content-Type': 'application/json', 'X-Ayati-Request': 'identity' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const data = await response.json();
  if (response.status === 401) throw new SignedOut(data.error || 'Sign in again.');
  if (!response.ok) throw new Error(data.error || 'Request could not be completed.');
  return data;
}

export const initials = (name: string) =>
  name.split(/[\s@._-]+/).filter(Boolean).slice(0, 2).map(part => Array.from(part)[0]).join('').toUpperCase() || 'W';

const TILES = ['#2E5BBA', '#2F7D5B', '#B4532A', '#7A4DB3', '#8A5A2B', '#1F7A8C'];
/** A stable colour per workspace, so its tile looks the same everywhere. */
export function tileColor(id: string) {
  let hash = 0;
  for (const char of id) hash = (hash * 31 + char.charCodeAt(0)) >>> 0;
  return TILES[hash % TILES.length];
}

export const days = (seconds: number) => Math.max(0, Math.ceil((seconds * 1000 - Date.now()) / 86_400_000));

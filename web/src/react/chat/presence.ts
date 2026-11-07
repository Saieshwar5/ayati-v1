// Who is online and the statuses people set (D245), in one store for the whole
// workspace. It is fed by the Chat directory and by the live stream's `presence`
// events (src/identity/chat/sync.rs), so it never polls. Each person's entry
// carries the server's revision: an older read never replaces a newer one.
import { useSyncExternalStore } from 'react';
import { workspaceId, workspaceSession } from '../../workspace-context';

export type StatusKind = 'working' | 'busy' | 'meeting' | 'away' | 'custom';
export type Status = { kind: StatusKind; emoji?: string | null; text?: string | null; clear_at?: number | null; set_at?: number | null };
/** What the server shows of one person; agents never have one. */
export type Presence = { id: string; online: boolean; last_seen: number | null; status: Status | null; revision: number };

const people = new Map<string, Presence>();
const listeners = new Set<() => void>();
/** The account and workspace the entries belong to. */
let scope = '';
let expiry: ReturnType<typeof setTimeout> | undefined;

const currentScope = () => `${workspaceSession?.user?.id ?? ''}/${workspaceId}`;
const publish = () => listeners.forEach(listener => listener());

/** The status while it lasts; its end is checked here as on the server. */
export function activeStatus(presence: Presence | undefined, now = Date.now()): Status | null {
  const status = presence?.status;
  return status && (status.clear_at == null || status.clear_at > now) ? status : null;
}

/** Shown with a green dot: online and not showing themselves as Away. */
export function shownOnline(presence: Presence | undefined, now = Date.now()): boolean {
  return Boolean(presence?.online) && activeStatus(presence, now)?.kind !== 'away';
}

function valid(entry: unknown): entry is Presence {
  const value = entry as Presence | null;
  return value !== null && typeof value === 'object' && typeof value.id === 'string' && typeof value.revision === 'number' && typeof value.online === 'boolean';
}

/** A status that has ended is dropped as it arrives, so the entry reads as the server would. */
function settled(entry: Presence, now: number): Presence {
  return entry.status && !activeStatus(entry, now) ? { ...entry, status: null } : entry;
}

/** Wakes subscribers when the next status ends, so its emoji goes without a reload. */
function scheduleExpiry() {
  clearTimeout(expiry);
  const now = Date.now();
  const next = Math.min(...[...people.values()].map(entry => entry.status?.clear_at ?? Infinity).filter(at => at > now));
  if (!Number.isFinite(next)) return;
  expiry = setTimeout(() => {
    const at = Date.now();
    for (const [id, entry] of people) { const current = settled(entry, at); if (current !== entry) people.set(id, current); }
    publish(); scheduleExpiry();
  }, Math.min(next - now + 50, 2 ** 31 - 1));
}

/** Merges presence read for `workspace`. Anything for another workspace or
 * account, malformed, or older than what is shown is ignored. */
export function applyPresence(list: unknown, workspace = workspaceId) {
  if (!Array.isArray(list) || !workspace || workspace !== workspaceId) return;
  if (scope !== currentScope()) { people.clear(); scope = currentScope(); }
  const now = Date.now();
  let changed = false;
  for (const entry of list) {
    if (!valid(entry)) continue;
    const next = settled(entry, now), known = people.get(next.id);
    if (known && (known.revision > next.revision || JSON.stringify(known) === JSON.stringify(next))) continue;
    people.set(next.id, next); changed = true;
  }
  if (changed) { scheduleExpiry(); publish(); }
}

/** Forgets everyone, after access to the workspace was refused. */
export function clearPresence() {
  clearTimeout(expiry);
  if (!people.size) return;
  people.clear(); publish();
}

export function subscribePresence(listener: () => void) { listeners.add(listener); return () => { listeners.delete(listener); }; }
export const presenceOf = (id: string | undefined) => (id && scope === currentScope() ? people.get(id) : undefined);
/** The signed-in person's own entry: their status and what others see. */
export const ownPresence = () => presenceOf(workspaceSession?.user?.id);

/** One person's presence; only components showing that person rerender when it changes. */
export function usePresence(id: string | undefined): Presence | undefined {
  return useSyncExternalStore(subscribePresence, () => presenceOf(id));
}

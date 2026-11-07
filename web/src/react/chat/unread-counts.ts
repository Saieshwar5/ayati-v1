// One shared store for the workspace's unread counts (D245). The workspace
// application starts it once; the rail's Chat badge, the tab title, the Chat
// list and desktop alerts all read it, so there is a single `/unread` reader.
import { useSyncExternalStore } from 'react';
import { identityRequest, workspaceId, workspaceSession } from '../../workspace-context';
import { subscribeChanges, type Change } from './live';
import type { Conversation } from './model';
import type { Level, NotifyDefaults } from './notifications';

export type UnreadCounts = { counts: Record<string, number>; mentions: Record<string, number> };
/** What decides how a conversation counts: its kind and the person's own level. */
export type ConversationSettings = { chats: ReadonlyMap<string, Conversation>; levels: Readonly<Record<string, Level>>; defaults: NotifyDefaults };
type UnreadResponse = { unread: { chat: string; count: number; mentions?: number }[] };
type Identity = { workspace: string; user: string | undefined };
type Run = { reloadConversations: () => Promise<void>; loaded: Promise<void> };

const noCounts: UnreadCounts = { counts: {}, mentions: {} };
const DEFAULTS: NotifyDefaults = { desktop_alerts: true, channel_level: 'mentions', approval_alerts: true };
const noSettings: ConversationSettings = { chats: new Map(), levels: {}, defaults: DEFAULTS };
let counts = noCounts, settings = noSettings, active: Run | undefined;
const listeners = new Set<() => void>();
const publish = () => listeners.forEach(fn => fn());

export const unreadCounts = () => counts;
export const conversationSettings = () => settings;
export function subscribeUnread(fn: () => void) { listeners.add(fn); return () => { listeners.delete(fn); }; }
/** Counts and settings are separate snapshots, so a reader of one does not rerender for the other. */
export const useUnreadCounts = () => useSyncExternalStore(subscribeUnread, unreadCounts);
export const useConversationSettings = () => useSyncExternalStore(subscribeUnread, conversationSettings);

/** Keep the badge and alerts in step when the person changes a level or a default on this page. */
export function rememberLevel(chat: string, level: Level) { settings = { ...settings, levels: { ...settings.levels, [chat]: level } }; publish(); }
export function setNotifyDefaults(defaults: NotifyDefaults) { settings = { ...settings, defaults }; publish(); }
/** Resolves once the current workspace's conversations and levels were first read. */
export const conversationsLoaded = () => active?.loaded ?? Promise.resolve();
/** Reads the conversations and levels again, for a conversation not listed yet. */
export const reloadConversations = () => active?.reloadConversations() ?? Promise.resolve();

/** Whether a live change can alter the viewer's unread counts. A message change
 * from someone else may be a new, finished or deleted message, or the viewer's
 * own seen receipt (receipts arrive as changes to their message, without saying
 * whose). Access changes cover archive, visibility and membership. Receipts and
 * reactions on the viewer's own messages, agent pauses and panel, sample or
 * event access never change what the viewer has unread. */
export function changesUnread(change: Change, viewer: string | undefined): boolean {
  if (change.kind === 'access' || change.kind === 'reset') return true;
  return change.kind === 'message' && change.message?.author !== viewer;
}

const denied = (e: unknown) => e instanceof Error && 'status' in e && [400, 401, 403, 404].includes(Number(e.status));

/** Runs `read` one at a time. A call during a read runs it once more afterwards,
 * so an older answer never replaces a newer one. `read` handles its own errors. */
function serially(live: () => boolean, read: () => Promise<void>): () => Promise<void> {
  let pending: Promise<void> | undefined, again = false;
  return () => {
    if (pending) { again = true; return pending; }
    pending = (async () => {
      try { do { again = false; await read(); } while (again && live()); }
      finally { pending = undefined; }
    })();
    return pending;
  };
}

/** Starts the store for one person in one workspace and returns its cleanup.
 * Counts are read again 500 ms after a change that can affect them, and the
 * conversations and levels after an access change or reset. Starting for
 * another workspace or person clears everything at once, and answers for the
 * previous one are ignored. */
export function startUnreadCounts(identity: Identity = { workspace: workspaceId, user: workspaceSession?.user?.id }) {
  if (!identity.workspace || !identity.user) return;
  const abort = new AbortController(), viewer = identity.user;
  const run = {} as Run;
  const live = () => active === run && !abort.signal.aborted;
  const request = <T>(path: string) => identityRequest<T>(`/workspaces/${identity.workspace}${path}`, undefined, abort.signal);
  active = run; counts = noCounts; settings = noSettings; publish();
  // Conversations /unread names that the list did not have yet (a new direct message).
  let listed = false;
  const asked = new Set<string>();

  const readConversations = serially(live, async () => {
    try {
      const [list, notify] = await Promise.all([
        request<{ chats: Conversation[] }>('/chats'),
        // Without saved levels the defaults still apply.
        request<{ levels?: Record<string, Level>; defaults?: NotifyDefaults }>('/chat-notifications').catch(() => undefined),
      ]);
      if (!live()) return;
      settings = { chats: new Map(list.chats.map(chat => [chat.id, chat])), levels: notify?.levels ?? settings.levels, defaults: notify?.defaults ?? settings.defaults };
      publish();
    } catch (e) {
      // Denied or removed access clears them; an outage keeps the last ones.
      if (live() && denied(e)) { settings = noSettings; publish(); }
    } finally { listed = true; }
  });
  const readCounts = serially(live, async () => {
    try {
      const data = await request<UnreadResponse>('/unread');
      if (!live()) return;
      counts = {
        counts: Object.fromEntries(data.unread.map(item => [item.chat, item.count])),
        mentions: Object.fromEntries(data.unread.map(item => [item.chat, item.mentions ?? 0])),
      };
      publish();
      const unknown = data.unread.map(item => item.chat).filter(chat => !settings.chats.has(chat) && !asked.has(chat));
      if (listed && unknown.length) { unknown.forEach(chat => asked.add(chat)); void readConversations(); }
    } catch (e) {
      if (live() && denied(e)) { counts = noCounts; publish(); }
    }
  });

  let countsTimer: number | undefined, listTimer: number | undefined;
  const unsubscribe = subscribeChanges(change => {
    if ((change.kind === 'access' || change.kind === 'reset') && listTimer === undefined) {
      listTimer = window.setTimeout(() => { listTimer = undefined; void readConversations(); }, 500);
    }
    if (changesUnread(change, viewer) && countsTimer === undefined) {
      countsTimer = window.setTimeout(() => { countsTimer = undefined; void readCounts(); }, 500);
    }
  });
  run.reloadConversations = readConversations;
  run.loaded = readConversations();
  void readCounts();
  return () => {
    abort.abort(); unsubscribe(); window.clearTimeout(countsTimer); window.clearTimeout(listTimer);
    if (active === run) { active = undefined; counts = noCounts; settings = noSettings; publish(); }
  };
}

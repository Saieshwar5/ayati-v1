// Desktop alerts for new messages, decided in the browser from each person's
// own levels. The server says which messages are for you (mentions and replies
// to your requests); levels never change who can read a conversation.
import { storageScope, workspaceSession, workspaceUrl } from '../../workspace-context';
import { navigate } from '../../navigation';
import { subscribeChanges, type Change } from './live';
import { chatRequest, type Conversation, type Message } from './model';

export type Level = 'all' | 'mentions' | 'none';
export const defaultLevel = (chat: Pick<Conversation, 'kind'>): Level => chat.kind === 'direct' ? 'all' : 'mentions';

const levels = new Map<string, Level>();
const chats = new Map<string, Conversation>();
const alerted = new Set<string>();
const RECENT_SECONDS = 120;

/** Keep alerts in step when the person changes a level on this page. */
export function rememberLevel(chat: string, level: Level) { levels.set(chat, level); }

export const alertsSupported = () => typeof window !== 'undefined' && 'Notification' in window;

/** Plain text for an alert: mention links become @names, Markdown marks go. */
export function alertText(content: string): string {
  const text = content.replace(/\[(@[^\]]+)\]\(mention:[^)]+\)/g, '$1').replace(/[*_`#>]+/g, '').replace(/\s+/g, ' ').trim();
  return text.length > 140 ? `${text.slice(0, 137)}…` : text;
}

export function shouldAlert(message: Message, chat: Conversation | undefined, level: Level, viewing: boolean, now = Date.now() / 1000): boolean {
  if (!chat || viewing || level === 'none') return false;
  if (message.author === workspaceSession?.user?.id) return false;
  if (message.state !== (message.kind === 'agent' ? 'complete' : 'sent')) return false;
  if (now - message.at > RECENT_SECONDS) return false;
  return level === 'all' || Boolean(message.for_me);
}

async function load() {
  try {
    const [list, saved] = await Promise.all([
      chatRequest<{ chats: Conversation[] }>('/chats'),
      chatRequest<{ levels: Record<string, Level> }>('/chat-notifications'),
    ]);
    chats.clear(); list.chats.forEach(chat => chats.set(chat.id, chat));
    levels.clear(); Object.entries(saved.levels).forEach(([chat, level]) => levels.set(chat, level));
  } catch { /* Without the lists no alert is shown; the next change retries. */ }
}

function viewingConversation(chat: string) {
  const url = new URL(window.location.href);
  return document.visibilityState === 'visible' && document.hasFocus()
    && url.searchParams.get('view') === 'chat' && url.searchParams.get('conversation') === chat;
}

function show(change: Change) {
  const message = change.message, id = change.chat;
  if (!message || !id || alerted.has(message.id)) return;
  const chat = chats.get(id);
  const level = levels.get(id) ?? (chat ? defaultLevel(chat) : 'none');
  if (!shouldAlert(message, chat, level, viewingConversation(id))) return;
  alerted.add(message.id);
  const author = message.name ?? 'Someone';
  const alert = new Notification(chat!.kind === 'direct' ? author : `${author} in #${chat!.name}`, {
    body: message.state === 'deleted' ? '' : alertText(message.content) || 'Sent a file', tag: message.id, icon: '/brand/cuckoo-symbol.svg',
  });
  alert.onclick = () => { window.focus(); navigate(workspaceUrl(`/?view=chat&conversation=${id}&message=${message.id}`)); alert.close(); };
}

/** Started once for the workspace; returns its cleanup. */
export function startDesktopAlerts() {
  if (!alertsSupported() || !workspaceSession?.user) return;
  let stopped = false, reload: number | undefined;
  void load();
  const unsubscribe = subscribeChanges(change => {
    if (stopped) return;
    if (change.kind === 'access' || change.kind === 'reset') { window.clearTimeout(reload); reload = window.setTimeout(() => void load(), 500); return; }
    if (change.kind === 'message' && Notification.permission === 'granted') show(change);
  });
  return () => { stopped = true; unsubscribe(); window.clearTimeout(reload); };
}

const dismissedKey = () => storageScope('chat.alerts-prompt-dismissed');
export function promptDismissed() { try { return localStorage.getItem(dismissedKey()) === '1'; } catch { return false; } }
export function dismissPrompt() { try { localStorage.setItem(dismissedKey(), '1'); } catch { /* The prompt returns next visit. */ } }

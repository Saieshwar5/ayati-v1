// Desktop alerts for new messages, decided in the browser from each person's
// own levels. The server says which messages are for you (mentions and replies
// to your requests); levels never change who can read a conversation.
import { storageScope, workspaceSession, workspaceUrl } from '../../workspace-context';
import { navigate } from '../../navigation';
import { subscribeChanges, type Change } from './live';
import type { Conversation, Message } from './model';
import { loadAttention } from '../attention/model';
import { conversationSettings, conversationsLoaded, reloadConversations, setNotifyDefaults as storeDefaults } from './unread-counts';
// The conversations, levels and defaults are read once for the workspace by the
// shared unread store, which also keeps them current after access changes.
export { rememberLevel } from './unread-counts';

export type Level = 'all' | 'mentions' | 'none';
/** The person's own defaults (Settings → Preferences), stored on the server. */
export type NotifyDefaults = { desktop_alerts: boolean; channel_level: Level; approval_alerts: boolean };
export function setNotifyDefaults(next: NotifyDefaults | { desktop_alerts: boolean; channel_level: string; approval_alerts: boolean }) { storeDefaults(next as NotifyDefaults); }
export const defaultLevel = (chat: Pick<Conversation, 'kind'>, defaults = conversationSettings().defaults): Level => chat.kind === 'direct' ? 'all' : defaults.channel_level;

const alerted = new Set<string>();
const RECENT_SECONDS = 120;

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

function viewingConversation(chat: string) {
  const url = new URL(window.location.href);
  return document.visibilityState === 'visible' && document.hasFocus()
    && url.searchParams.get('view') === 'chat' && url.searchParams.get('conversation') === chat;
}

async function show(change: Change) {
  const message = change.message, id = change.chat;
  if (!conversationSettings().defaults.desktop_alerts || !message || !id || alerted.has(message.id)) return;
  // A new conversation's first message can arrive before the list refreshes.
  if (!conversationSettings().chats.has(id)) await reloadConversations();
  if (alerted.has(message.id)) return;
  const { chats, levels } = conversationSettings(), chat = chats.get(id);
  const level = levels[id] ?? (chat ? defaultLevel(chat) : 'none');
  if (!shouldAlert(message, chat, level, viewingConversation(id))) return;
  alerted.add(message.id);
  const author = message.name ?? 'Someone';
  const alert = new Notification(chat!.kind === 'direct' ? author : `${author} in #${chat!.name}`, {
    body: message.state === 'deleted' ? '' : alertText(message.content) || 'Sent a file', tag: message.id, icon: '/brand/cuckoo-symbol.svg',
  });
  alert.onclick = () => { window.focus(); navigate(workspaceUrl(`/?view=chat&conversation=${id}&message=${message.id}`)); alert.close(); };
}

/** Approvals the person may decide that they have already been told about. */
const knownApprovals = new Set<string>();
async function approvals(silent: boolean) {
  const { defaults } = conversationSettings();
  if (!defaults.desktop_alerts || !defaults.approval_alerts) return;
  try {
    const page = await loadAttention('needs_you', null);
    for (const item of page.items) {
      if (item.kind !== 'approval' || item.status === 'uncertain' || knownApprovals.has(item.id)) continue;
      knownApprovals.add(item.id);
      if (silent || item.read) continue;
      const alert = new Notification(`${item.agent_name} needs an approval`, { body: item.title.slice(0, 140), tag: `approval-${item.id}`, icon: '/brand/cuckoo-symbol.svg' });
      alert.onclick = () => { window.focus(); navigate(workspaceUrl('/?view=activity&tab=needs')); alert.close(); };
    }
  } catch { /* The next approval change retries. */ }
}

/** Started once for the workspace, after the unread store; returns its cleanup. */
export function startDesktopAlerts() {
  if (!alertsSupported() || !workspaceSession?.user) return;
  let stopped = false, approval: number | undefined;
  // Approvals already waiting when the page opens are not announced again.
  void conversationsLoaded().then(() => { if (!stopped) void approvals(true); });
  const unsubscribe = subscribeChanges(change => {
    if (stopped || change.kind !== 'message' || Notification.permission !== 'granted') return;
    void show(change);
    if (change.message?.progress?.state === 'approval') { window.clearTimeout(approval); approval = window.setTimeout(() => void approvals(false), 800); }
  });
  return () => { stopped = true; unsubscribe(); window.clearTimeout(approval); };
}

const dismissedKey = () => storageScope('chat.alerts-prompt-dismissed');
export function promptDismissed() { try { return localStorage.getItem(dismissedKey()) === '1'; } catch { return false; } }
export function dismissPrompt() { try { localStorage.setItem(dismissedKey(), '1'); } catch { /* The prompt returns next visit. */ } }

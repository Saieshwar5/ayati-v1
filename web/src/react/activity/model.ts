import { identityRequest, storageScope, workspaceId, workspaceUrl } from '../../workspace-context';

/** Filters shared by History, its totals and an agent's Activity tab. */
/** Whose agents: `mine` (agents you manage and your own requests) or `all` (everything you may see). */
export type Scope = 'mine' | 'all';
export type Filters = { agent?: string; person?: string; chat?: string; status?: string; kind?: string; from?: number; to?: number; q?: string; scope?: Scope };
export type Step = { id: string; title: string; effect: string; state: string; automatic: boolean; decision: string | null; decided_by: string | null; account: string | null; arguments: Record<string, unknown>; at: number };
export type ActivityItem = {
  id: string; chat: string; thread: string | null; conversation_name: string; conversation_kind: string; in_conversation: boolean;
  agent: string; agent_name: string; agent_avatar: string | null; kind: 'chat' | 'schedule' | 'trigger' | 'handoff';
  requester: string; requester_name: string | null; summary: string; state: string; latest_update: string;
  created_ms: number; finished_ms: number | null; can_cancel: boolean; approvals: { id: string; title: string; state: string }[];
};
/** The schedule a run came from, and whether its result was posted (schema 56). */
export type RunSchedule = { id: string; name: string; agent: string; state: string; revision: number; principal: string; interval_seconds: number | null;
  post_to: string | null; posted: 'posted' | 'skipped' | null; post_reason: string | null;
  /** The run happened in the schedule's own work room, which isn't a conversation. */
  room: boolean };
export type Details = ActivityItem & { message: string | null; reply: string; steps: Step[]; fix: string | null; disagreement: string | null; schedule?: RunSchedule | null };
export type Totals = {
  requests: number; done: number; failed: number; review: number; waiting: number; automatic: number; approved: number;
  people: number; channels: number; schedule_runs: number;
  per_day: { day: string; count: number }[]; by_agent: { agent: string; name: string; avatar: string | null; count: number }[];
};
export type AgentCard = { id: string; name: string; role: string; avatar: string | null; paused: boolean; paused_note: string | null;
  running: number; waiting: number; today: number; failed_today: number; doing: string | null; next_run: number | null;
  /** Today's latest request that could not complete, for "How to fix it". */
  last_failed?: string | null;
  /** The viewer owns or manages it, or is an admin, so may pause or resume it. */
  can_pause?: boolean };
export type Now = { agents: AgentCard[]; running: ActivityItem[]; away: { since: number; totals: Totals }; agents_paused?: boolean };
export type Change = { id: number; at: number; agent: string; agent_name: string; agent_avatar: string | null; summary: string; actor_name: string };

const root = `/workspaces/${workspaceId}/activity`;
function query(filters: Filters, extra: Record<string, string | number | undefined> = {}) {
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries({ ...filters, ...extra })) if (value !== undefined && value !== '') params.set(key, String(value));
  return params.toString();
}
export const loadHistory = (filters: Filters, before?: number, signal?: AbortSignal) =>
  identityRequest<{ items: ActivityItem[]; next_before: number | null }>(`${root}/history?${query(filters, { before })}`, undefined, signal);
export const loadTotals = (filters: Filters, signal?: AbortSignal) =>
  identityRequest<Totals>(`${root}/totals?${query(filters, { offset: -new Date().getTimezoneOffset() })}`, undefined, signal);
export const loadChanges = (agent?: string, before?: number, signal?: AbortSignal) =>
  identityRequest<{ changes: Change[]; next_before: number | null }>(`${root}/changes?${query({ agent }, { before })}`, undefined, signal);
/** History as CSV for the same filters (at most 5,000 requests). */
export const exportUrl = (filters: Filters) => `/api/identity${root}/export?${query(filters)}`;
export const startOfToday = () => { const d = new Date(); d.setHours(0, 0, 0, 0); return d.getTime(); };
export const loadNow = (scope: Scope, signal?: AbortSignal) => identityRequest<Now>(`${root}/now?today=${startOfToday()}&scope=${scope}`, undefined, signal);
/** Pause or resume one agent (its owner, managers and admins). */
export const pauseAgent = (agent: string, paused: boolean) =>
  identityRequest<{ paused: boolean; revision: number }>(`/workspaces/${workspaceId}/agent-profiles/${encodeURIComponent(agent)}/pause`, { paused });
/** Ask a failed request again, as the person who asked. Repeats give the same retry. */
export const retryRequest = (reply: string) =>
  identityRequest<{ reply: string; chat: string; retry_of: string }>(`${root}/requests/${encodeURIComponent(reply)}/retry`, {});
const scopeKey = () => storageScope('activity.scope');
/** The last chosen scope on this device; My agents until the person picks another. */
export function savedScope(): Scope {
  try { return localStorage.getItem(scopeKey()) === 'all' ? 'all' : 'mine'; } catch { return 'mine'; }
}
export function saveScope(scope: Scope) { try { localStorage.setItem(scopeKey(), scope); } catch { /* Only a convenience. */ } }
export const markSeen = () => identityRequest<{ since: number | null }>(`${root}/seen`, {});
export const loadDetails = (reply: string, signal?: AbortSignal) => identityRequest<Details>(`${root}/requests/${encodeURIComponent(reply)}`, undefined, signal);
export const stopRequest = (item: ActivityItem) => identityRequest(`/workspaces/${workspaceId}/chats/${encodeURIComponent(item.chat)}/messages/${encodeURIComponent(item.id)}/cancel`, {});
export const conversationUrl = (item: { chat: string; id: string; thread: string | null }) =>
  workspaceUrl(`/?view=chat&conversation=${encodeURIComponent(item.chat)}&message=${encodeURIComponent(item.id)}${item.thread ? `&thread=${encodeURIComponent(item.thread)}` : ''}`);

/** One plain status for a request, and its tone. */
export function status(item: { state: string; approvals?: { state: string }[] }): { label: string; tone: 'done' | 'working' | 'waiting' | 'problem' | 'quiet' } {
  if (item.approvals?.some(a => a.state === 'uncertain')) return { label: 'Unconfirmed', tone: 'waiting' };
  switch (item.state) {
    case 'complete': return { label: 'Done', tone: 'done' };
    case 'failed': case 'interrupted': return { label: 'Could not complete', tone: 'problem' };
    case 'cancelled': return { label: 'Stopped', tone: 'quiet' };
    case 'approval': return { label: 'Waiting for approval', tone: 'waiting' };
    case 'question': case 'waiting': return { label: 'Waiting', tone: 'waiting' };
    case 'access_changed': return { label: 'Access changed', tone: 'problem' };
    case 'queued': case 'preparing': case 'resuming': return { label: 'Starting', tone: 'working' };
    default: return { label: 'Working', tone: 'working' };
  }
}
export const kindLabels: Record<ActivityItem['kind'], string> = { chat: 'Chat', schedule: 'Schedule', trigger: 'Trigger', handoff: 'Hand-off' };
export const where = (item: ActivityItem) => item.kind === 'schedule' ? `Schedule · ${item.conversation_name}` : item.kind === 'trigger' ? `trigger in ${item.conversation_name}`
  : item.conversation_kind === 'direct' ? 'direct chat' : `#${item.conversation_name}`;
export const clock = (ms: number) => new Date(ms).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
export function dayLabel(ms: number): string {
  const day = new Date(ms); day.setHours(0, 0, 0, 0);
  const today = startOfToday();
  if (day.getTime() === today) return 'Today';
  if (day.getTime() === today - 86400000) return 'Yesterday';
  return day.toLocaleDateString([], { weekday: 'short', day: 'numeric', month: 'short' });
}
export function ago(ms: number): string {
  const minutes = Math.max(0, Math.round((Date.now() - ms) / 60000));
  if (minutes < 1) return 'just now';
  if (minutes < 60) return `${minutes} min`;
  const hours = Math.round(minutes / 60);
  return hours < 24 ? `${hours} h` : dayLabel(ms).toLowerCase();
}

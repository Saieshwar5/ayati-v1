import { identityRequest, storageScope, workspaceId, workspaceUrl } from '../../workspace-context';

/** Filters shared by History, its totals and an agent's Activity tab. */
/** Whose agents: `mine` (agents you manage and your own requests) or `all` (everything you may see). */
export type Scope = 'mine' | 'all';
export type Filters = { agent?: string; person?: string; chat?: string; status?: string; kind?: string; from?: number; to?: number; q?: string; scope?: Scope };
export type Step = { id: string; title: string; effect: string; state: string; automatic: boolean; decision: string | null; decided_by: string | null; account: string | null; arguments: Record<string, unknown>; at: number };
export type ActivityItem = {
  id: string; chat: string; thread: string | null; conversation_name: string; conversation_kind: string; in_conversation: boolean;
  agent: string; agent_name: string; agent_avatar: string | null; kind: 'chat' | 'schedule' | 'trigger' | 'handoff' | 'panel';
  /** A Home panel refresh names its panel. */
  panel?: { id: string; title: string } | null;
  /** The task this request works towards, if any. */
  task?: TaskRef | null;
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
  /** How many Needs fixing groups this agent is in (Phase 4). */
  needs_fixing?: number;
  /** The viewer owns or manages it, or is an admin, so may pause or resume it. */
  can_pause?: boolean };
export type Now = { agents: AgentCard[]; running: ActivityItem[]; away: { since: number; totals: Totals }; agents_paused?: boolean;
  /** Events that arrived today but no agent acted on. */
  unhandled_today?: number;
  /** What's wrong across the agents this person can fix, grouped by cause (Phase 4). */
  fixing?: FixGroup[];
  /** Routines, follow-ups and due tasks in the next 24 hours. */
  coming_up?: ComingItem[] };
export type FixKind = 'request' | 'needs' | 'tasks' | 'agent' | 'settings' | 'need';
export type FixGroup = { cause: string; title: string; count: number; fix: { kind: FixKind; label: string; ref?: string };
  agents: { id: string; name: string }[];
  items: { agent: string; agent_name: string; ref: string; detail: string | null; at: number | null; cause?: string; request?: string; fix?: string | null }[] };
export type ComingItem = { kind: 'routine' | 'follow_up' | 'due'; id: string; agent: string; agent_name: string; title: string; at: number; revision?: number; can_pause: boolean };
export const dismissNeed = (id: string) => identityRequest(`${root}/needs/${encodeURIComponent(id)}/dismiss`, {});
export const cancelFollowUp = (task: string) => identityRequest(`${root}/tasks/${encodeURIComponent(task)}/follow-up/cancel`, {});
/** An event no agent acted on, with the recorded reason; never the event's contents. */
export type Unhandled = { id: string; sequence: number; at: number; trigger: string; title: string; reason: string | null;
  agent: string; agent_name: string; agent_avatar: string | null };
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
export const loadUnhandled = (params: { from?: number; scope?: Scope; agent?: string }, before?: number, signal?: AbortSignal) =>
  identityRequest<{ items: Unhandled[]; next_before: number | null }>(`${root}/unhandled?${query({ from: params.from, scope: params.scope, agent: params.agent }, { before })}`, undefined, signal);
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
export const kindLabels: Record<ActivityItem['kind'], string> = { chat: 'Chat', schedule: 'Schedule', trigger: 'Trigger', handoff: 'Hand-off', panel: 'Panel update' };
export const where = (item: ActivityItem) => item.kind === 'panel' ? 'Home panel' : item.kind === 'schedule' ? `Schedule · ${item.conversation_name}` : item.kind === 'trigger' ? `trigger in ${item.conversation_name}`
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

/** A History row: one request, or one panel's routine updates on one day. */
export type HistoryEntry = { kind: 'item'; item: ActivityItem } | { kind: 'panel-fold'; key: string; items: ActivityItem[] };
/** Routine Home panel updates fold into one line per panel per day, at the newest
 * one, so hourly refreshes don't bury people's requests. Updates that went wrong
 * stay on their own line. */
export function foldPanelUpdates(items: ActivityItem[]): HistoryEntry[] {
  const entries: HistoryEntry[] = [];
  const folds = new Map<string, ActivityItem[]>();
  for (const item of items) {
    if (item.kind === 'panel' && item.panel && status(item).tone !== 'problem') {
      const key = `${item.panel.id}.${dayLabel(item.created_ms)}`;
      const fold = folds.get(key);
      if (fold) { fold.push(item); continue; }
      const group = [item]; folds.set(key, group);
      entries.push({ kind: 'panel-fold', key, items: group });
    } else entries.push({ kind: 'item', item });
  }
  return entries.map(entry => entry.kind === 'panel-fold' && entry.items.length === 1 ? { kind: 'item', item: entry.items[0] } : entry);
}

/** A task: a promise with an outcome (people-and-agents-first, Phase 2). */
export type TaskStatus = 'open' | 'working' | 'waiting' | 'needs_fixing' | 'scheduled' | 'late' | 'done' | 'failed' | 'cancelled' | 'not_task';
export type TaskRef = { id: string; title: string; kind: 'automatic' | 'commitment'; status: TaskStatus };
export type TaskItem = TaskRef & {
  origin: string; done_when: string | null; waiting_on: 'approval' | 'question' | 'helper' | null;
  agent: string; agent_name: string; agent_avatar: string | null; requester: string; requester_name: string | null;
  chat: string; conversation_name: string; conversation_kind: string; created_ms: number; start_ms: number | null; due_ms: number | null;
  closed_ms: number | null; outcome: string | null; result: string | null; steps: number;
  /** The agent's next booked follow-up, and why the last one couldn't run. */
  next_wakeup_ms?: number | null; blocked_wakeup?: string | null;
};
export type TaskDetail = TaskItem & { may_close: boolean; requests: { id: string; state: string; created_ms: number; summary: string }[] };
export const taskLabels: Record<TaskStatus, string> = {
  open: 'Open', working: 'Working', waiting: 'Waiting', needs_fixing: 'Needs fixing', scheduled: 'Scheduled', late: 'Late',
  done: 'Done', failed: 'Failed', cancelled: 'Cancelled', not_task: 'Not a task',
};
export const taskTone = (status: TaskStatus) => status === 'needs_fixing' || status === 'late' || status === 'failed' ? 'problem'
  : status === 'waiting' ? 'waiting' : status === 'working' ? 'working' : status === 'done' ? 'done' : 'quiet';
export const waitingLabels: Record<string, string> = { approval: 'an approval', question: 'an answer', helper: 'another agent' };
export const loadTasks = (scope: Scope, signal?: AbortSignal) =>
  identityRequest<{ items: TaskItem[]; more: boolean }>(`${root}/tasks?scope=${scope}`, undefined, signal);
export const loadTask = (id: string, signal?: AbortSignal) => identityRequest<TaskDetail>(`${root}/tasks/${encodeURIComponent(id)}`, undefined, signal);
export const closeTask = (id: string, outcome: 'done' | 'cancelled' | 'not_task') =>
  identityRequest<{ id: string; outcome: string }>(`${root}/tasks/${encodeURIComponent(id)}/close`, { outcome });
export const makeTask = (reply: string) => identityRequest<{ id: string; title: string }>(`${root}/requests/${encodeURIComponent(reply)}/task`, {});
export const taskUrl = (id: string) => workspaceUrl(`/?view=activity&tab=open&task=${encodeURIComponent(id)}`);

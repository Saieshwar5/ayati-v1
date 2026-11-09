import { identityRequest, workspaceId } from '../../workspace-context';
import { changeSchedule } from '../agents/automation/model';
import { cancelFollowUp, dayLabel, type Scope } from './model';

/** Activity › Upcoming (designs/automations-simplified 3): what agents will do on
 * their own. `own` is true for what this person set up; only they resume it. */
type Base = { id: string; agent: string; agent_name: string; title: string };
export type UpcomingSchedule = Base & { kind: 'schedule'; at: number | null; state: 'enabled' | 'paused' | 'blocked'; revision: number;
  interval_seconds: number | null; timezone: string; detail: string; by: string; own: boolean };
export type UpcomingTrigger = Base & { kind: 'trigger'; source: string | null; state: 'enabled' | 'paused' | 'blocked'; revision: number; detail: string; by: string; own: boolean };
export type UpcomingAppTrigger = Base & { kind: 'app_trigger'; app: string; app_name: string; change: 'added' | 'changed'; by: string | null; manages: boolean };
export type UpcomingFollowUp = Base & { kind: 'follow_up'; at: number };
export type UpcomingDue = Base & { kind: 'due'; at: number };
export type UpcomingTask = UpcomingFollowUp | UpcomingDue;
export type UpcomingItem = UpcomingSchedule | UpcomingTrigger | UpcomingAppTrigger | UpcomingFollowUp | UpcomingDue;

export const loadUpcoming = (scope: Scope) => identityRequest<{ items: UpcomingItem[] }>(`/workspaces/${workspaceId}/activity/upcoming?scope=${scope}`);

export type Filter = 'all' | 'schedules' | 'triggers' | 'follow_ups';
export const filters: [Filter, string][] = [['all', 'All'], ['schedules', 'Schedules'], ['triggers', 'Triggers'], ['follow_ups', 'Follow-ups']];
const inFilter = (item: UpcomingItem, filter: Filter) => filter === 'all'
  || (filter === 'schedules' && item.kind === 'schedule')
  || (filter === 'triggers' && (item.kind === 'trigger' || item.kind === 'app_trigger'))
  || (filter === 'follow_ups' && (item.kind === 'follow_up' || item.kind === 'due'));

/** Items grouped for reading: by day of their next time, then paused ones, then
 * triggers, which are always on. Derived on each render from the server list. */
export function grouped(items: UpcomingItem[], filter: Filter, search: string): [string, UpcomingItem[]][] {
  const query = search.trim().toLowerCase();
  const shown = items.filter(item => inFilter(item, filter) && (!query || `${item.title} ${item.agent_name}`.toLowerCase().includes(query)));
  const timed = shown.filter((item): item is UpcomingSchedule | UpcomingTask =>
    (item.kind === 'schedule' && item.state === 'enabled' && item.at !== null) || item.kind === 'follow_up' || item.kind === 'due')
    .sort((a, b) => (a.at ?? 0) - (b.at ?? 0));
  const groups: [string, UpcomingItem[]][] = [];
  for (const item of timed) {
    const day = dayLabel(item.at ?? 0);
    const last = groups.at(-1);
    if (last?.[0] === day) last[1].push(item); else groups.push([day, [item]]);
  }
  const paused = shown.filter(item => item.kind === 'schedule' && (item.state !== 'enabled' || item.at === null));
  if (paused.length) groups.push(['Paused', paused]);
  const always = shown.filter(item => item.kind === 'trigger' || item.kind === 'app_trigger');
  if (always.length) groups.push(['Always on', always]);
  return groups;
}

/** Pause or resume a schedule, or pause a trigger; resuming a trigger needs its
 * access reviewed again, which happens on the agent. Each is one explicit request. */
export async function setRunning(item: UpcomingSchedule | UpcomingTrigger, enabled: boolean, revision = item.revision) {
  if (item.kind === 'schedule') return changeSchedule(item.agent, item.id, { request: crypto.randomUUID(), revision, enabled });
  return identityRequest<{ revision: number }>(`/workspaces/${workspaceId}/agent-profiles/${encodeURIComponent(item.agent)}/event-triggers/${encodeURIComponent(item.id)}/state`,
    { request: crypto.randomUUID(), revision, enabled: false, connection_revision: null, read_revision: null, delegation_revision: null });
}
export const cancel = (item: UpcomingTask) => cancelFollowUp(item.id);

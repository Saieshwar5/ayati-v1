import { identityRequest, workspaceId } from '../../workspace-context';
import { cancelFollowUp, dayLabel, type Scope } from './model';

/** Activity › Calendar › List (designs/automations-simplified 3): what agents will do
 * on their own. Schedules became tasks in schema 147: their runs are booked task
 * runs here. `own` is true for a trigger this person set up. */
type Base = { id: string; agent: string; agent_name: string; title: string };
export type UpcomingTrigger = Base & { kind: 'trigger'; source: string | null; state: 'enabled' | 'paused' | 'blocked'; revision: number; detail: string; by: string; own: boolean };
export type UpcomingFollowUp = Base & { kind: 'follow_up'; at: number };
export type UpcomingDue = Base & { kind: 'due'; at: number };
export type UpcomingTask = UpcomingFollowUp | UpcomingDue;
export type UpcomingItem = UpcomingTrigger | UpcomingFollowUp | UpcomingDue;

export const loadUpcoming = (scope: Scope) => identityRequest<{ items: UpcomingItem[] }>(`/workspaces/${workspaceId}/activity/upcoming?scope=${scope}`);

export type Filter = 'all' | 'triggers' | 'follow_ups';
export const filters: [Filter, string][] = [['all', 'All'], ['triggers', 'Triggers'], ['follow_ups', 'Follow-ups']];
const inFilter = (item: UpcomingItem, filter: Filter) => filter === 'all'
  || (filter === 'triggers' && item.kind === 'trigger')
  || (filter === 'follow_ups' && (item.kind === 'follow_up' || item.kind === 'due'));

/** Items grouped for reading: by day of their next time, then triggers, which are
 * always on. Derived on each render from the server list. */
export function grouped(items: UpcomingItem[], filter: Filter, search: string): [string, UpcomingItem[]][] {
  const query = search.trim().toLowerCase();
  const shown = items.filter(item => inFilter(item, filter) && (!query || `${item.title} ${item.agent_name}`.toLowerCase().includes(query)));
  const timed = shown.filter((item): item is UpcomingTask => item.kind === 'follow_up' || item.kind === 'due')
    .sort((a, b) => a.at - b.at);
  const groups: [string, UpcomingItem[]][] = [];
  for (const item of timed) {
    const day = dayLabel(item.at);
    const last = groups.at(-1);
    if (last?.[0] === day) last[1].push(item); else groups.push([day, [item]]);
  }
  const always = shown.filter(item => item.kind === 'trigger');
  if (always.length) groups.push(['Always on', always]);
  return groups;
}

/** Pause a trigger; resuming it needs its access reviewed again, which happens on
 * the agent. One explicit request. */
export async function pauseTrigger(item: UpcomingTrigger) {
  return identityRequest<{ revision: number }>(`/workspaces/${workspaceId}/agent-profiles/${encodeURIComponent(item.agent)}/event-triggers/${encodeURIComponent(item.id)}/state`,
    { request: crypto.randomUUID(), revision: item.revision, enabled: false, connection_revision: null, read_revision: null, delegation_revision: null });
}
export const cancel = (item: UpcomingTask) => cancelFollowUp(item.id);

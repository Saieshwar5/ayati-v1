import { identityRequest, workspaceId } from '../../workspace-context';
import type { Scope, TaskRunState } from './model';

/** Activity › Calendar (designs/agent-tasks): the runs agents made and will make,
 * in the workspace's time zone, plus what has no time (triggers, paused tasks). */
type Base = { agent: string; agent_name: string; title: string };
export type CalendarRun = Base & { kind: 'run'; task: string; at: number; state: Exclude<TaskRunState, 'planned'>; detail: string; reply: string | null };
export type CalendarPlanned = Base & { kind: 'planned'; task: string; at: number; run_kind: 'repeat' | 'follow_up' | 'continue' };
/** A schedule's runs from before schema 147 come as runs of the task it became. */
export type CalendarItem = CalendarRun | CalendarPlanned;
/** A trigger links to its task (schema 148), whose runs show as the task's runs. */
export type Untimed = Base & ({ kind: 'trigger'; trigger: string; task: string | null; detail: string | null } | { kind: 'paused'; task: string; detail: string | null });
export type CalendarData = { from: number; to: number; now: number; timezone: string; items: CalendarItem[]; untimed: Untimed[] };

export const loadCalendar = (scope: Scope, from: number, to: number) =>
  identityRequest<CalendarData>(`/workspaces/${workspaceId}/activity/calendar?from=${from}&to=${to}&scope=${scope}`);

const DAY = 86_400_000;

/** Monday 00:00 (device time) of the week `offset` weeks from the one holding `now`. */
export function weekStart(now: number, offset: number): number {
  const date = new Date(now);
  date.setHours(0, 0, 0, 0);
  const back = (date.getDay() + 6) % 7;
  date.setDate(date.getDate() - back + offset * 7);
  return date.getTime();
}

/** The date of `ms` in `zone`, as a key like 2026-11-24. */
export function dayKey(ms: number, zone: string): string {
  const parts = new Intl.DateTimeFormat('en-CA', { timeZone: zone, year: 'numeric', month: '2-digit', day: '2-digit' }).formatToParts(new Date(ms));
  const part = (type: string) => parts.find(p => p.type === type)?.value ?? '';
  return `${part('year')}-${part('month')}-${part('day')}`;
}

export const timeIn = (ms: number, zone: string) =>
  new Intl.DateTimeFormat('en-GB', { timeZone: zone, hour: '2-digit', minute: '2-digit', hourCycle: 'h23' }).format(new Date(ms));

/** The seven days shown, each with its items in time order. The window is asked
 * for a day wider on each side, so a different device time zone loses nothing. */
export function days(data: CalendarData, start: number): { key: string; name: string; date: string; items: CalendarItem[] }[] {
  const week = Array.from({ length: 7 }, (_, i) => start + i * DAY + DAY / 2);
  return week.map(ms => {
    const key = dayKey(ms, data.timezone);
    const name = new Intl.DateTimeFormat('en-GB', { timeZone: data.timezone, weekday: 'short' }).format(new Date(ms));
    const date = new Intl.DateTimeFormat('en-GB', { timeZone: data.timezone, day: 'numeric', month: 'short' }).format(new Date(ms));
    return { key, name, date, items: data.items.filter(item => dayKey(item.at, data.timezone) === key) };
  });
}

export const range = (start: number) => ({ from: start - DAY, to: start + 8 * DAY });

/** What a past or planned item reads as. */
export function itemState(item: CalendarItem): { label: string; tone: 'planned' | 'done' | 'problem' | 'quiet' | 'working' } {
  if (item.kind === 'planned') return { label: item.run_kind === 'continue' ? 'Next part' : 'Planned', tone: 'planned' };
  const labels: Record<string, [string, 'done' | 'problem' | 'quiet' | 'working']> = {
    done: ['Done', 'done'], no_change: ['No change', 'quiet'], failed: ['Failed', 'problem'], blocked: ["Couldn't start", 'problem'],
    missed: ['Missed', 'problem'], skipped: ['Skipped', 'quiet'], cancelled: ['Stopped', 'quiet'], working: ['Working', 'working'],
  };
  const [label, tone] = labels[item.state] ?? ['Done', 'done'];
  return { label, tone };
}

export const zoneLabel = (zone: string) => zone === 'Asia/Kolkata' ? 'India time' : zone.replace(/_/g, ' ');

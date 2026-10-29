import { chatRequest } from '../../chat/model';
import { workspaceUrl } from '../../../workspace-context';
import type { BusinessClock } from '../../../business-time';
export type ScheduleInput = { id: string; chat: string; name: string; content: string; first_ms: number; interval_seconds: number | null; timezone: string };
export type Routine = ScheduleInput & { principal: string; principal_name: string; chat_name: string; revision: number; state: 'enabled' | 'paused' | 'blocked' | 'completed'; next_ms: number | null; detail: string; unfinished?: boolean; instructions_long?: boolean; can_resume: boolean };
export type Directory = { schedules: Routine[]; conversations: { id: string; name: string; kind: string }[]; more_conversations: boolean; execution_available: boolean };
export type Run = { due_ms: number; message: string; reply: string; chat: string; state: string; job_state: string; progress?: { state: string; text: string } | null; created_ms: number };
const path = (agent: string) => `/agent-profiles/${encodeURIComponent(agent)}/schedules`;
export const loadSchedules = (agent: string) => chatRequest<Directory>(path(agent));
export const loadSchedule = (agent: string, id: string) => chatRequest<Routine>(`${path(agent)}/${encodeURIComponent(id)}`);
export const saveSchedule = (agent: string, input: ScheduleInput) => chatRequest<Routine>(path(agent), input);
export const changeSchedule = (agent: string, id: string, input: { request: string; revision: number; enabled: boolean }) => chatRequest<{ id: string; state: string; revision: number }>(`${path(agent)}/${encodeURIComponent(id)}/state`, input);
export const loadRuns = (agent: string, id: string) => chatRequest<{ runs: Run[] }>(`${path(agent)}/${encodeURIComponent(id)}/runs`);
/** Schedules use the viewer's device timezone until a shared business clock exists. */
export function deviceClock(): BusinessClock {
  return { location: 'Device time', timezone: Intl.DateTimeFormat().resolvedOptions().timeZone, hour12: true };
}
export function date(ms: number, timezone: string) {
  try { return new Date(ms).toLocaleString(undefined, { timeZone: timezone, dateStyle: 'medium', timeStyle: 'short' }); }
  catch { return `${new Date(ms).toISOString()} (UTC)`; }
}
export const cadence = (seconds: number | null) => seconds === null ? 'Run once' : seconds % 86400 === 0 ? `Every ${seconds / 86400} 24-hour period${seconds === 86400 ? '' : 's'}` : seconds % 3600 === 0 ? `Every ${seconds / 3600} hour${seconds === 3600 ? '' : 's'}` : seconds % 60 === 0 ? `Every ${seconds / 60} minute${seconds === 60 ? '' : 's'}` : `Every ${seconds} seconds`;
export const runUrl = (run: Run) => workspaceUrl(`/?view=chat&conversation=${encodeURIComponent(run.chat)}&message=${encodeURIComponent(run.reply)}`);

import { identityRequest, workspaceId } from '../../../workspace-context';
import { subscribeChanges } from '../../chat/live';
import { defaults, type Agent } from '../model';
import type { ResponseReference, ResultSource } from '../../results/model';
export type Source = { id: string; revision: number; access_revision: number; title: string; kind: string; visibility?: string; readiness?: string };
export type Note = { id: string; title: string; revision: number };
export type SampleState = 'queued' | 'running' | 'completed' | 'failed' | 'cancelled' | 'interrupted' | 'invalidated';
export type SampleInput = { id: string; agent_revision: number; task: string; sources: Pick<Source, 'id' | 'revision' | 'access_revision'>[] };
export type Sample = { id: string; state: SampleState; detail: string; input: SampleInput; profile: Agent;
  sources: Source[]; note_refs: Note[]; created_ms: number; started_ms: number | null; finished_ms: number | null;
  preview?: string; response?: ResponseReference };
export type Options = { profile: Agent; sources: Source[]; more: boolean; notes: Note[]; execution_available: boolean; active_sample: { id: string; state: SampleState } | null };
export type History = { samples: { id: string; state: SampleState; detail: string; task: string; profile_revision: number; created_ms: number }[]; next_before: string | null };
const root = (agent: string) => `/workspaces/${workspaceId}/agent-profiles/${encodeURIComponent(agent)}`;
export async function loadOptions(agent: string, search = '', signal?: AbortSignal) {
  const data = await identityRequest<Options>(`${root(agent)}/rehearsal-options?${new URLSearchParams({ search })}`, undefined, signal);
  return { ...data, profile: { ...data.profile, settings: { ...defaults, ...data.profile.settings } } };
}
export function loadHistory(agent: string, before: string | null) {
  return identityRequest<History>(`${root(agent)}/rehearsals${before ? `?${new URLSearchParams({ before })}` : ''}`);
}
export const loadSample = (agent: string, id: string, signal?: AbortSignal) => identityRequest<Sample>(`${root(agent)}/rehearsals/${encodeURIComponent(id)}`, undefined, signal);
export const startSample = (agent: string, input: SampleInput) => identityRequest<Sample>(`${root(agent)}/rehearsals`, input);
export const stopSample = (agent: string, id: string) => identityRequest<{ id: string; state: SampleState }>(`${root(agent)}/rehearsals/${encodeURIComponent(id)}/stop`, {});
export const activeSample = (state?: string) => state === 'queued' || state === 'running';
export const sampleLabel = (state: string) => ({ queued: 'Queued', running: 'Working on sample', completed: 'Draft saved', failed: 'Sample failed', cancelled: 'Stopped', interrupted: 'Execution interrupted', invalidated: 'Access or knowledge changed' }[state] ?? 'Unavailable');
export function sampleSource(agent: string): ResultSource {
  return { read: (path, signal) => identityRequest(path, undefined, signal), href: path => `/api/identity${path}`,
    response: id => `${root(agent)}/rehearsals/${encodeURIComponent(id)}/response`, file: () => { throw new Error('Samples do not publish files.'); },
    subscribe: check => subscribeChanges(change => { if (change.kind === 'access' || change.kind === 'reset' || change.kind === 'sample_access') check(); }) };
}

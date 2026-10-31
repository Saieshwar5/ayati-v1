import { identityRequest, workspaceId } from '../../workspace-context';
export const base = `/workspaces/${workspaceId}/agent-profiles`;
export type Settings = { provider: string; instructions: string; escalation: string; successful_outcome?: string; routine?: string; avatar: string; memory: boolean; browser: boolean; history: string; paused: boolean };
export const defaults: Settings = { provider: '', instructions: '', escalation: '', successful_outcome: '', routine: '', avatar: 'spark', memory: false, browser: false, history: 'all', paused: false };
/** Who can talk to an agent (D231): everyone in the workspace, selected people, or only its owner. */
export type Audience = 'everyone' | 'selected' | 'only_me';
export type Agent = { id: string; name: string; role: string; model: string; audience: Audience; revision: number; settings: Settings; creator_name?: string; owner_name?: string; owner_id?: string; editable?: boolean; can_talk?: boolean; people?: string[]; managers?: string[]; pending?: number };
export const audienceLabels: Record<Audience, string> = { everyone: 'Everyone in the workspace', selected: 'Selected people', only_me: 'Only me' };
export type Directory = { agents: Agent[]; role: string; default_model: string; system_prompt: string; providers: { id: string; name: string; configured: boolean }[] };
export const loadAgents = () => identityRequest<Directory>(base);
export function saveAgent(agent: Agent) {
  const { id, name, role, model, audience, people, managers, revision, settings } = agent;
  return identityRequest<Agent>(base, { id, name: name.trim(), role: role.trim(), model, audience, people, managers, revision, settings });
}
/** Saves the profile, then an uploaded picture. A failed upload keeps the saved
 * profile and its revision, so a retry never creates the agent twice. */
export async function saveAgentWithPicture(draft: Agent, file: File | undefined, keep: (saved: Agent) => void) {
  let next = await saveAgent(draft);
  keep(next);
  if (!file) return next;
  const response = await fetch(`/api/identity${base}/${draft.id}/avatar`, { method: 'POST', headers: { 'X-Ayati-Request': 'identity' }, body: file });
  const result = await response.json();
  if (!response.ok) throw new Error(result.error || 'Picture upload failed. The agent was saved.');
  next = await saveAgent({ ...next, people: undefined, managers: undefined, settings: { ...next.settings, avatar: result.avatar } });
  keep(next);
  return next;
}
/** Exactly what an agent with this draft receives, including the Cuckoo rules. */
export const previewPrompt = (draft: { name: string; role: string; instructions: string }, signal?: AbortSignal) =>
  identityRequest<{ prompt: string }>(`${base}/prompt`, draft, signal);
export const symbols: Record<string, string> = { spark: '✦', diamond: '◈', flower: '✿', sun: '☀' };

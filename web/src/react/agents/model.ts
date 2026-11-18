import { identityRequest, workspaceId } from '../../workspace-context';
import type { Catalog } from './catalog-model';
import type { ComputerOffer, ComputerSettings } from './computer/model';
export const base = `/workspaces/${workspaceId}/agent-profiles`;
export type Settings = { provider: string; instructions: string; escalation: string; successful_outcome?: string; routine?: string; avatar: string; memory: boolean; browser: boolean; web_search?: boolean; computer?: ComputerSettings; history: string; paused: boolean;
  /** What it handles, one line, and whether other agents may ask it for help (schema 124). */
  handles?: string; helper?: boolean };
export const defaults: Settings = { provider: '', instructions: '', escalation: '', successful_outcome: '', routine: '', avatar: 'spark', memory: false, browser: false, web_search: true, history: 'all', paused: false };
/** Who can talk to an agent (D231): everyone in the workspace, selected people, or only its owners. */
export type Audience = 'everyone' | 'selected' | 'only_me';
export type Agent = { id: string; name: string; role: string; model: string; audience: Audience; revision: number; settings: Settings; creator_name?: string; owner_name?: string; owner_id?: string; editable?: boolean; can_talk?: boolean; people?: string[]; pending?: number;
  /** Its owners, first added first: equal people who each change it, decide its
   * approvals and see all its work (schema 122). `is_owner`: the viewer is one. */
  owners?: Owner[]; is_owner?: boolean;
  /** A member's own assistant (schema 127): private to them, with no other owners. */
  assistant?: boolean;
  /** Its last owner left the workspace: it stays paused until an admin gives it one. */
  needs_owner?: boolean };
export type Owner = { id: string; name: string };
/** "A", "A and B", "A, B and C", or "A and 3 others". */
export function ownerNames(owners: Owner[]): string {
  const names = owners.map(owner => owner.name);
  if (names.length > 3) return `${names[0]} and ${names.length - 1} others`;
  return names.length > 1 ? `${names.slice(0, -1).join(', ')} and ${names.at(-1)}` : names[0] ?? '';
}
/** Adds or removes one owner; the reply has the owners and the agent's new revision. */
export const changeOwners = (agent: string, change: { add: string } | { remove: string }, revision: number) =>
  identityRequest<{ owners: Owner[]; revision: number }>(`${base}/${agent}/owners`, { ...change, revision });
export const audienceLabels: Record<Audience, string> = { everyone: 'Everyone in the workspace', selected: 'Selected people', only_me: 'Only me' };
export type Directory = { agents: Agent[]; role: string; default_model: string; models?: { provider: string; model: string }[]; system_prompt: string; providers: { id: string; name: string; configured: boolean }[]; catalog?: Catalog; web_search?: boolean; computers?: ComputerOffer | null };
export const loadAgents = () => identityRequest<Directory>(base);
export function saveAgent(agent: Agent) {
  const { id, name, role, model, audience, people, revision, settings } = agent;
  return identityRequest<Agent>(base, { id, name: name.trim(), role: role.trim(), model, audience, people, revision, settings });
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
  next = await saveAgent({ ...next, people: undefined, settings: { ...next.settings, avatar: result.avatar } });
  keep(next);
  return next;
}
/** What decides the preview: a saved `agent`, or for a new one its chosen apps and skills. */
export type PromptDraft = { name: string; role: string; instructions: string; agent?: string; apps?: boolean; skills?: { name: string; description: string }[] };
export type PromptSection = { id: string; title: string; state: 'included' | 'not_included' | 'conversation'; reason: string; text: string };
export type PromptPreview = { prompt: string; version: string; sections: PromptSection[]; instructions: string; skills: { name: string; description: string }[] };
/** Everything an agent with this draft receives: the Cuckoo system prompt in
 * sections, and the exact text of a direct request. */
export const previewPrompt = (draft: PromptDraft, signal?: AbortSignal) =>
  identityRequest<PromptPreview>(`${base}/prompt`, draft, signal);
export const symbols: Record<string, string> = { spark: '✦', diamond: '◈', flower: '✿', sun: '☀' };

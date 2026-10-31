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
  const { id, name, role, model, audience, revision, settings } = agent;
  return identityRequest<Agent>(base, { id, name: name.trim(), role: role.trim(), model, audience, revision, settings });
}
export const symbols: Record<string, string> = { spark: '✦', diamond: '◈', flower: '✿', sun: '☀' };

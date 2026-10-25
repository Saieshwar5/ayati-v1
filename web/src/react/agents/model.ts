import { identityRequest, workspaceId } from '../../workspace-context';
export const base = `/workspaces/${workspaceId}/agent-profiles`;
export type Settings = { provider: string; instructions: string; escalation: string; avatar: string; memory: boolean; browser: boolean; history: string; paused: boolean };
export const defaults: Settings = { provider: '', instructions: '', escalation: '', avatar: 'spark', memory: false, browser: false, history: 'all', paused: false };
export type Agent = { id: string; name: string; role: string; model: string; scope: 'private' | 'workspace'; revision: number; settings: Settings; creator_name?: string; owner_name?: string; editable?: boolean; pending?: number };
export type Directory = { agents: Agent[]; role: string; can_create_workspace?: boolean; default_model: string; system_prompt: string; providers: { id: string; name: string; configured: boolean }[] };
export const loadAgents = () => identityRequest<Directory>(base);
export function saveAgent(agent: Agent) {
  const { id, name, role, model, scope, revision, settings } = agent;
  return identityRequest<Agent>(base, { id, name: name.trim(), role: role.trim(), model, scope, revision, settings });
}
export const symbols: Record<string, string> = { spark: '✦', diamond: '◈', flower: '✿', sun: '☀' };

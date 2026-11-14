import { identityRequest, workspaceId } from '../../../workspace-context';
import type { Mode } from '../control-model';

/** One tool a connected system lists, as Cuckoo reviewed it (C1, D249). */
export type ListedTool = {
  name: string; title: string; description: string;
  /** `read` or `write` from the tool's own hints, when it gives them. */
  suggested: 'read' | 'write' | null;
  supported: boolean; reason: string | null; fields: string[];
};
export type ToolChoice = { name: string; kind: 'read' | 'write'; rule: Mode };
export type SystemState = 'connected' | 'changed' | 'unreachable' | 'off';
export type System = {
  id: string; name: string; url: string; has_key: boolean; tools: ToolChoice[]; revision: number;
  state: SystemState; held: string[]; last_call_ms: number | null; created_by: string | null;
};
export type Systems = { systems: System[]; can_connect: boolean };

const path = (agent: string, rest = '') => `/workspaces/${workspaceId}/agent-profiles/${encodeURIComponent(agent)}/systems${rest}`;
export const loadSystems = (agent: string) => identityRequest<Systems>(path(agent));
export const previewSystem = (agent: string, url: string, key: string) =>
  identityRequest<{ tools: ListedTool[] }>(path(agent, '/preview'), { url, ...(key ? { key } : {}) });
export const connectSystem = (agent: string, input: { id: string; name: string; url: string; key?: string; tools: ToolChoice[] }) =>
  identityRequest<System>(path(agent), input);
export const savedTools = (agent: string, id: string) => identityRequest<{ tools: ListedTool[] }>(path(agent, `/${id}/tools`), {});
export const reviewSystem = (agent: string, id: string, input: { revision: number; tools: ToolChoice[]; key?: string }) =>
  identityRequest<System>(path(agent, `/${id}/review`), input);
export const changeSystem = (agent: string, id: string, revision: number, change: 'on' | 'off' | 'remove') =>
  identityRequest<{ id: string; state: string }>(path(agent, `/${id}/state`), { revision, change });

export const stateLabels: Record<SystemState, string> = {
  connected: 'Connected', changed: 'Changed since it was reviewed', unreachable: 'Cannot reach', off: 'Off',
};
/** The starting rule for a tool: reads are allowed, changes and unknowns ask first. */
export const startingRule = (kind: 'read' | 'write' | null): Mode => kind === 'read' ? 'allow' : 'ask';
/** Choices for every supported tool, keeping any saved review, with the first 20 on. */
export function initialChoices(tools: ListedTool[], saved: ToolChoice[] = []): Record<string, ToolChoice & { on: boolean }> {
  const out: Record<string, ToolChoice & { on: boolean }> = {};
  let on = 0;
  for (const tool of tools.filter(t => t.supported)) {
    const kept = saved.find(s => s.name === tool.name);
    const kind = kept?.kind ?? tool.suggested ?? 'write';
    const turnOn = saved.length ? Boolean(kept) : on < 20;
    if (turnOn) on += 1;
    out[tool.name] = { name: tool.name, kind, rule: kept?.rule ?? startingRule(tool.suggested), on: turnOn };
  }
  return out;
}

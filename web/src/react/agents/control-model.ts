import { identityRequest, workspaceId } from '../../workspace-context';
import { base } from './model';

export type Mode = 'allow' | 'ask' | 'block';
/** One connector action with the admin's rule, the agent's own rule and the result. */
export type ToolOperation = { id: string; title: string; effect: 'read' | 'write'; workspace_mode: Mode; needs_other: boolean; agent_mode: Mode | null; mode: Mode; revision: number };
export type ToolConnection = { id: string; server: string; name?: string | null; account?: string | null; revision: number; on: boolean; operations: ToolOperation[] };
export type Action = { id: string; connection: string; operation: string; title: string; effect: string; arguments: unknown; state: string; result: unknown; chat: string; thread: string | null; job: string; approver: string; created: number; updated: number; requester_name?: string; conversation_name?: string; decision?: string | null; delivery?: { status: string; reason?: string | null } | null };
export type Controls = { can_manage: boolean; can_manage_connections: boolean; viewer: string; connections: ToolConnection[]; actions: Action[]; execution_available?: boolean };
export const controlPath = (agent: string) => `${base}/${agent}/control`;
export const loadControls = (agent: string) => identityRequest<Controls>(controlPath(agent));
/** Connectors a new agent could use, with each action's workspace rule (create step 4). */
export const loadAvailable = () => identityRequest<{ connections: ToolConnection[]; execution_available?: boolean }>(`/workspaces/${workspaceId}/control/available`);
export const switchTool = (agent: string, connection: string, enabled: boolean) => identityRequest(`${controlPath(agent)}/tools`, { connection, enabled });
export const saveRule = (agent: string, rule: { connection: string; operation: string; mode: Mode; revision: number }) => identityRequest(`${controlPath(agent)}/rules`, rule);
export const modes: Record<Mode, string> = { allow: 'Allow', ask: 'Ask first', block: 'Block' };
const rank: Record<Mode, number> = { allow: 0, ask: 1, block: 2 };
/** An agent's rule may only be as strict as the admin's rule, or stricter. */
export const allowedFor = (ceiling: Mode, mode: Mode) => rank[mode] >= rank[ceiling];
export const effectLabel = (effect: string) => effect === 'read' ? 'Reads' : 'Changes';
export const states: Record<string, string> = { pending: 'Waiting for approval', executing: 'Executing', completed: 'Completed', denied: 'Blocked', rejected: 'Rejected', invalidated: 'Permission changed', uncertain: 'Outcome unconfirmed' };

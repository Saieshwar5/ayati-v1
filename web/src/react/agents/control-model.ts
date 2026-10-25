import { identityRequest } from '../../workspace-context';
import { base } from './model';

export type Mode = 'allow' | 'ask' | 'block';
export type Operation = { id: string; title: string; tool?: string; effect: 'read' | 'write'; schema: unknown };
export type Server = { id: string; name: string; account?: string | null; operations: Operation[] };
export type Connection = { id: string; server: string; definition: Server; enabled: boolean; revision: number };
export type Rule = { connection: string; operation: string; mode: Mode; revision: number; approver_name: string; chat: string };
export type Action = { id: string; connection: string; operation: string; title: string; effect: string; arguments: unknown; state: string; result: unknown; chat: string; thread: string | null; job: string; approver: string; created: number; updated: number; requester_name?: string; conversation_name?: string; decision?: string | null };
export type Delegation = { connection: string; user_id: string; operation: string; ceiling: Mode; revision: number; approver: string; approver_name: string };
export type Controls = { can_manage: boolean; can_manage_connections: boolean; delegations: Delegation[]; viewer: string; connections: Connection[]; rules: Rule[]; actions: Action[]; servers: Server[]; conversations: { id: string; name: string }[]; execution_available?: boolean };
export const controlPath = (agent: string) => `${base}/${agent}/control`;
export const loadControls = (agent: string) => identityRequest<Controls>(controlPath(agent));
export const saveRule = (agent: string, rule: { connection: string; operation: string; mode: Mode; revision: number; chat: string; delegation_revision?: number }) => identityRequest(`${controlPath(agent)}/rules`, rule);
export const decideAction = (agent: string, action: string, id: string, decision: 'approve' | 'reject') => identityRequest<Action>(`${controlPath(agent)}/actions/${action}/decision`, { id, decision });
export const modes: Record<Mode, string> = { allow: 'Allow automatically', ask: 'Ask first', block: 'Block' };
export const states: Record<string, string> = { pending: 'Waiting for approval', executing: 'Executing', completed: 'Completed', denied: 'Blocked', rejected: 'Rejected', invalidated: 'Permission changed', uncertain: 'Outcome unconfirmed' };

import { identityRequest, workspaceId, workspaceUrl } from '../../workspace-context';
import { base } from './model';

export type AgentRequest = {
  source_scope?: string | null;
  coordination?: import('../chat/handoffs').Coordination;
  id: string; sequence: number; chat: string; thread: string | null; source_message: string | null;
  conversation_name: string; requester: string; requester_name: string | null;
  summary: string; state: string; latest_update: string; created_ms: number;
  started_ms: number | null; finished_ms: number | null; can_cancel: boolean;
  cancellation: { by: string; at: number } | null;
  approvals: { id: string; title: string; state: string }[];
};
export type RequestPage = { requests: AgentRequest[]; next_before: number | null };
export const requestLabels: Record<string, string> = {
  queued: 'Queued', preparing: 'Preparing', running: 'Working', recovering: 'Recovering',
  handoff: 'Waiting for another agent',
  approval: 'Waiting for approval', input: 'Waiting for an answer', waiting: 'Waiting',
  resuming: 'Ready to continue', attention: 'Needs attention', complete: 'Completed',
  failed: 'Could not complete', cancelled: 'Cancelled', access_changed: 'Access changed',
};
export const loadRequests = (agent: string, scope = 'mine', status = 'active', before?: number) =>
  identityRequest<RequestPage>(`${base}/${agent}/requests?scope=${scope}&status=${status}${before ? `&before=${before}` : ''}`);
export const loadRequest = (agent: string, reply: string) => identityRequest<AgentRequest>(`${base}/${agent}/requests/${reply}`);
export const cancelRequest = (request: AgentRequest) => identityRequest(`/workspaces/${workspaceId}/chats/${request.chat}/messages/${request.id}/cancel`, {});
export const requestUrl = (request: AgentRequest) => workspaceUrl(`/?view=chat&conversation=${encodeURIComponent(request.chat)}&message=${encodeURIComponent(request.id)}${request.thread ? `&thread=${encodeURIComponent(request.thread)}` : ''}`);

import { identityRequest, workspaceId } from '../../workspace-context';
import type { Action } from '../agents/control-model';
export type Approval = Action & { source_scope?: string | null; agent: string; agent_name: string; account_name: string; approver_name: string; can_decide: boolean; decision?: string | null };
export type Decision = 'approve' | 'reject' | 'request_changes';
const root = `/workspaces/${workspaceId}/control`;
export const loadApprovals = () => identityRequest<{ actions: Approval[] }>(`${root}/approvals`);
export const loadApproval = (id: string) => identityRequest<Approval>(`${root}/actions/${id}`);
export const decideApproval = (id: string, decision: Decision, requestId: string) => identityRequest<Approval>(`${root}/actions/${id}/decision`, { id: requestId, decision });

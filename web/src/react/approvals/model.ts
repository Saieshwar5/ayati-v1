import { identityRequest, workspaceId } from '../../workspace-context';
import type { Action } from '../agents/control-model';
export type Approval = Action & { source_scope?: string | null; agent: string; agent_name: string; account_name: string; approver_name: string; can_decide: boolean; decision?: string | null;
  /** Managers who can decide now (D231), when admins may step in, and who decided. */
  deciders?: { id: string; name: string }[]; backup_at?: number; needs_other?: boolean; decided_by_name?: string | null };
/** "Ravi", "Ravi or Meera", "Ravi, Meera or Arjun". */
export const either = (names: string[]) => names.length < 2 ? names.join('') : `${names.slice(0, -1).join(', ')} or ${names.at(-1)}`;
export type Decision = 'approve' | 'reject' | 'request_changes';
const root = `/workspaces/${workspaceId}/control`;
export const loadApproval = (id: string) => identityRequest<Approval>(`${root}/actions/${id}`);
export const decideApproval = (id: string, decision: Decision, requestId: string) => identityRequest<Approval>(`${root}/actions/${id}/decision`, { id: requestId, decision });

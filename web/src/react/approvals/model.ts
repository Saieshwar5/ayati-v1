import { identityRequest, workspaceId } from '../../workspace-context';
import type { Action } from '../agents/control-model';
export type Approval = Action & { source_scope?: string | null; agent: string; agent_name: string; account_name: string; approver_name: string; can_decide: boolean; decision?: string | null;
  /** Managers who can decide now (D231), when admins may step in, and who decided. */
  deciders?: { id: string; name: string }[]; backup_at?: number; needs_other?: boolean; decided_by_name?: string | null;
  /** An unconfirmed write (schema 107): who may answer, what Cuckoo checked, and the answer. */
  settle?: Settle | null; settled?: Settled | null };
export type Settle = { can_settle: boolean; checks: { at: number; result: 'unknown' | 'happened' | 'not_happened' }[] };
export type Settled = { by: string; by_name: string; at: number; happened: boolean; note: string };
/** "Ravi", "Ravi or Meera", "Ravi, Meera or Arjun". */
export const either = (names: string[]) => names.length < 2 ? names.join('') : `${names.slice(0, -1).join(', ')} or ${names.at(-1)}`;
export type Decision = 'approve' | 'reject' | 'request_changes';
const root = `/workspaces/${workspaceId}/control`;
export const loadApproval = (id: string) => identityRequest<Approval>(`${root}/actions/${id}`);
/** Says whether an unconfirmed write happened, which lets the account's writes run again. */
export const settleOutcome = (id: string, answer: { happened: boolean; note: string }) => identityRequest<Approval>(`${root}/actions/${id}/outcome`, answer);
export const decideApproval = (id: string, decision: Decision, requestId: string) => identityRequest<Approval>(`${root}/actions/${id}/decision`, { id: requestId, decision });

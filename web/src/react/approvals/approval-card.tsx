import { useState } from 'react';
import { Button } from '../ui';
import { ApprovalDialog } from './approval-dialog';
import { useApprovalViewer } from './viewer';
import { states } from '../agents/control-model';

export type ApprovalReference = {
  id?: string; state?: string; restricted?: boolean;
  /** Pending only: this person may decide now (a manager, or an admin after the backup delay). */
  may_decide?: boolean;
  /** Pending only: the managers who could decide now. */
  waiting_for?: string[];
  /** Pending only: when admins may step in, in seconds. */
  backup_at?: number;
};

/** "Priya", "Priya or Meera", "Priya, Meera or Ravi". */
export function deciderNames(names: string[]): string {
  return names.length < 2 ? names[0] ?? '' : `${names.slice(0, -1).join(', ')} or ${names[names.length - 1]}`;
}

/** What the card says about a waiting approval, for the person looking at it. */
export function waitingLine(reference: ApprovalReference, now = Date.now()): string {
  if (reference.may_decide) return 'Waiting for your decision';
  const who = reference.waiting_for?.length ? `Waiting for approval from ${deciderNames(reference.waiting_for)}` : "Waiting for an admin's approval";
  const backup = reference.backup_at ? reference.backup_at * 1000 : 0;
  if (!backup || backup <= now || !reference.waiting_for?.length) return who;
  return `${who}. An admin can step in after ${new Date(backup).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}.`;
}

/** An approval request inside the conversation it came from. Everyone who can see
 * the work sees the card; only the people who may decide get the decision. Others
 * see whom it waits for and can read the details. Needs me always has it too. */
export function ApprovalCard({ reference }: { reference: ApprovalReference }) {
  const viewer = useApprovalViewer();
  const [open, show] = useState(false);
  const pending = reference.state === 'pending' && reference.may_decide !== undefined;
  const decides = pending && reference.may_decide;
  const label = reference.restricted ? 'Details are restricted to people with access to the source work.'
    : pending ? waitingLine(reference) : states[reference.state ?? ''] ?? 'Saved request';
  const action = decides ? 'Review and decide' : pending ? 'Details' : 'Review saved request';
  return <section className="approval-chat-card" aria-label="Saved approval request">
    <strong>Approval request</strong>
    <p>{label}</p>
    {reference.id && <Button primary={Boolean(decides)} onClick={() => { if (viewer) viewer(reference.id!); else show(true); }}>{action}</Button>}
    {!viewer && open && reference.id && <ApprovalDialog id={reference.id} close={() => show(false)} />}
  </section>;
}

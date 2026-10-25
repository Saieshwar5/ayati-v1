import { useState } from 'react';
import { Button } from '../ui';
import { ApprovalDialog } from './approval-dialog';
import { states } from '../agents/control-model';
export type ApprovalReference = { id?: string; state?: string; restricted?: boolean };
export function ApprovalCard({ reference }: { reference: ApprovalReference }) {
  const [open, show] = useState(false);
  return <section className="approval-chat-card" aria-label="Saved approval request">
    <strong>Approval request</strong>
    <p>{reference.restricted ? 'Details are restricted to people with access to the source work.' : states[reference.state ?? ''] ?? 'Saved request'}</p>
    {reference.id && <Button onClick={() => show(true)}>Review saved request</Button>}
    {open && reference.id && <ApprovalDialog id={reference.id} close={() => show(false)} />}
  </section>;
}

import { useEffect, useState } from 'react';
import { Button, Dialog } from '../ui';
import { useResource } from '../requests';
import { loadApprovals } from './model';
import { ApprovalDialog } from './approval-dialog';
import { states } from '../agents/control-model';

export function ApprovalInbox({ close }: { close: () => void }) {
  const { data, error, refresh } = useResource(loadApprovals);
  const [selected, select] = useState('');
  useEffect(() => { const timer = window.setInterval(() => { if (!document.hidden) void refresh().catch(() => {}); }, 5000); return () => clearInterval(timer); }, [refresh]);
  if (selected) return <ApprovalDialog key={selected} id={selected} close={() => { select(''); void refresh().catch(() => {}); }} />;
  return <Dialog drawer title="My approvals" close={close} status={error}>
    <p className="ui-help">Up to 50 requests you are currently allowed to view, with waiting requests first. Only the designated authorized person can decide.</p>
    {!data && !error && <p role="status">Loading approvals…</p>}
    {data && !data.actions.length && <p>No approval requests available to you.</p>}
    {data?.actions.map(action => <article className="approval-inbox-row" key={action.id}>
      <strong>{action.title}</strong><p>{action.agent_name} · {action.account_name}</p>
      <p>{action.decision === 'request_changes' ? 'Changes requested' : states[action.state] ?? action.state}</p>
      <Button aria-label={`Review ${action.title}`} onClick={() => select(action.id)}>Review request</Button>
    </article>)}
    <Button onClick={() => void refresh().catch(() => {})}>Refresh approvals</Button>
  </Dialog>;
}

import { useCallback, useEffect, useRef } from 'react';
import { Button, Dialog } from '../ui';
import { useMutation, useResource } from '../requests';
import { workspaceUrl } from '../../workspace-context';
import { ApprovalFields } from './fields';
import { decideApproval, loadApproval, type Decision } from './model';
import { states } from '../agents/control-model';
import './approvals.css';
import { subscribeChanges } from '../chat/live';

export function ApprovalDialog({ id, close }: { id: string; close: () => void }) {
  const load = useCallback(() => loadApproval(id), [id]);
  const { data, error, refresh, setData } = useResource(load);
  const mutation = useMutation();
  const identities = useRef(new Map<string, string>());
  useEffect(() => { const timer = window.setInterval(() => { if (!document.hidden) void refresh().catch(() => {}); }, 3000); return () => clearInterval(timer); }, [refresh]);
  useEffect(() => subscribeChanges(change => {
    if (['access','reset'].includes(change.kind) || (change.kind==='event_access' && (!data || (change.chat===data.chat && data.source_scope===change.source)))) {
      setData(undefined); void refresh().catch(() => {});
    }
  }),[refresh,setData,data?.chat,data?.source_scope]);
  const decide = (decision: Decision) => mutation.run(async () => {
    const key = `${id}/${decision}`, request = identities.current.get(key) ?? crypto.randomUUID();
    identities.current.set(key, request);
    await decideApproval(id, decision, request); await refresh();
  });
  return <Dialog drawer title="Review approval request" close={close} busy={mutation.busy} status={error || mutation.status}>
    {!data && !error && <p role="status">Loading authorized request details…</p>}
    {data && <section className="approval-detail" aria-label="Exact approval request">
      <h3>{data.title}</h3><p>{data.agent_name} · {data.account_name}</p>
      <p>{data.conversation_name} · Requested by {data.requester_name}</p>
      <p role="status">{data.decision === 'request_changes' ? 'Changes requested' : states[data.state] ?? data.state}</p>
      <p>Designated approver: <strong>{data.approver_name}</strong></p>
      <p className="ui-help">Approval applies once to these exact fields. Other channel members cannot decide for the designated person.</p>
      <ApprovalFields fields={data.arguments} />
      {data.can_decide && <div className="approval-buttons">
        <Button primary disabled={mutation.busy} onClick={() => void decide('approve')}>Approve this action</Button>
        <Button disabled={mutation.busy} onClick={() => void decide('reject')}>Reject</Button>
        <Button disabled={mutation.busy} onClick={() => void decide('request_changes')}>Request changes</Button>
      </div>}
      {data.state === 'pending' && !data.can_decide && <p>Waiting for the designated authorized approver.</p>}
      {data.decision === 'request_changes' && <p>Ask for a revised proposal in the work conversation. The previous fields have not been approved.</p>}
      {data.state === 'invalidated' && <p>Permissions or routing changed. A new action request is required.</p>}
      {data.state === 'uncertain' && <p>The external outcome is unconfirmed. Check the business system before another write.</p>}
      {data.state === 'completed' && <details><summary>Confirmed result</summary><pre>{JSON.stringify(data.result, null, 2)}</pre></details>}
      <a href={workspaceUrl(`/?view=chat&conversation=${encodeURIComponent(data.chat)}${data.thread ? `&thread=${encodeURIComponent(data.thread)}` : ''}`)}>Open work conversation</a>
      <details><summary>Request receipt</summary><small>{data.id}</small></details>
    </section>}
    <Button disabled={mutation.busy} onClick={() => void refresh().catch(() => {})}>Refresh request</Button>
  </Dialog>;
}

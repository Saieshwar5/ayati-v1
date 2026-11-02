import { useEffect, useRef, useState } from 'react';
import { Button, Dialog } from '../ui';
import { errorMessage, useMutation } from '../requests';
import { ApprovalFields } from '../approvals/fields';
import { decideApproval, loadApproval, type Approval, type Decision } from '../approvals/model';
import type { AttentionItem } from '../attention/model';

/** Needs me at volume (Phase 4 AC5): several requests for the same action from the
 * same agent, reviewed on one screen. Each one is still decided on its own: the
 * server checks every decision, and "Approve all" sends them one by one. */
export function ReviewTogether({ items, close }: { items: AttentionItem[]; close: () => void }) {
  const [loaded, setLoaded] = useState<Record<string, Approval | string>>({});
  const [done, setDone] = useState<Record<string, string>>({});
  const m = useMutation();
  const ids = useRef<Record<string, string>>({});
  useEffect(() => {
    let live = true;
    for (const item of items) loadApproval(item.id)
      .then(data => { if (live) setLoaded(old => ({ ...old, [item.id]: data })); })
      .catch(error => { if (live) setLoaded(old => ({ ...old, [item.id]: errorMessage(error) })); });
    return () => { live = false; };
  }, [items]);
  const decide = async (id: string, decision: Decision) => {
    ids.current[`${id}.${decision}`] ??= crypto.randomUUID();
    await decideApproval(id, decision, ids.current[`${id}.${decision}`]);
    setDone(old => ({ ...old, [id]: decision === 'approve' ? 'Approved' : 'Rejected' }));
  };
  const open = items.filter(item => !done[item.id] && typeof loaded[item.id] === 'object' && (loaded[item.id] as Approval).can_decide);
  const all = () => void m.run(async () => {
    let failed = 0;
    for (const item of open) { try { await decide(item.id, 'approve'); } catch { failed += 1; } }
    m.setStatus(failed ? `${open.length - failed} approved; ${failed} could not be decided. Open them one by one.` : `${open.length} approved, each on its own.`);
  });
  const title = items[0]?.operation_title ?? 'Requests';
  return <Dialog drawer title={`${items.length} × ${title}`} close={close} busy={m.busy} status={m.status}>
    <p>From {items[0]?.agent_name}. Each is decided separately; nothing runs until you decide it.</p>
    <ol className="activity-review-list">{items.map(item => {
      const data = loaded[item.id];
      return <li key={item.id}>
        {typeof data === 'string' ? <p role="alert">{data}</p> : !data ? <p role="status">Loading…</p> : <>
          <strong>{data.title}</strong>
          <ApprovalFields fields={data.arguments} />
          {done[item.id] ? <p role="status">{done[item.id]}</p> : data.can_decide ? <div className="activity-detail-actions">
            <Button primary disabled={m.busy} onClick={() => void m.run(() => decide(item.id, 'approve'))}>Approve</Button>
            <Button disabled={m.busy} onClick={() => void m.run(() => decide(item.id, 'reject'))}>Reject</Button>
          </div> : <small>You can't decide this one.</small>}
        </>}
      </li>;
    })}</ol>
    {open.length > 1 && <Button primary disabled={m.busy} onClick={all}>Approve all {open.length}</Button>}
  </Dialog>;
}

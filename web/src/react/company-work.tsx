import { useCallback, useState } from 'react';
import { chatRequest, type Directory } from './chat/model';
import { currentWorkspace, workspaceUrl } from '../workspace-context';
import { Button } from './ui';
import { useMutation, useResource } from './requests';
type Work = { id: string; chat: string; title: string; details: Record<string, unknown>; assignee: string; status: string; revision: number };
export function CompanyWork({ directory }: { directory: Directory }) {
  const resource = useResource(useCallback(() => chatRequest<{ work: Work[] }>('/work'), []));
  const mutation = useMutation();
  const [assignees, setAssignees] = useState<Record<string, string>>({});
  const manager = ['Owner', 'Admin'].includes(currentWorkspace()?.role ?? '');
  return <section><header><h2>Work records</h2><Button onClick={() => void resource.refresh().catch(() => {})}>Refresh work</Button></header>
    <p>Internal drafts, approvals and handoffs. Completed records are retained. No external order or payment is sent.</p>
    {(resource.error || mutation.status) && <p role="alert">{resource.error || mutation.status}</p>}
    <div className="company-rows">{!resource.error && resource.data?.work.map(item => <article key={item.id}><header><h3>{item.title}</h3><strong>{item.status}</strong></header><pre>{JSON.stringify(item.details, null, 2)}</pre>
      <a href={workspaceUrl(`/?view=chat&conversation=${item.chat}`)}>Open source conversation</a>
      <p>Responsible: {directory.users.find(u => u.id === item.assignee)?.name ?? 'Former member'}</p>
      {manager && item.status !== 'completed' && <><label>Handoff to<select value={assignees[item.id] ?? item.assignee} onChange={e => setAssignees({ ...assignees, [item.id]: e.target.value })}>{directory.users.map(u => <option value={u.id} key={u.id}>{u.name}</option>)}</select></label>
        {['draft', 'approved', 'rejected', ...(item.status === 'approved' ? ['completed'] : [])].map(status => <Button key={status} disabled={mutation.busy} onClick={() => void mutation.run(async () => { await chatRequest(`/work/${item.id}`, { status, assignee: assignees[item.id] ?? item.assignee, revision: item.revision }); await resource.refresh(); })}>{status === 'draft' ? 'Save handoff' : status === 'approved' ? 'Approve' : status === 'rejected' ? 'Reject' : 'Complete'}</Button>)}</>}
    </article>)}</div>
    {resource.data?.work.length === 0 && <p>No internal work records yet.</p>}
  </section>;
}

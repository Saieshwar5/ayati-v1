import { useCallback, useEffect, useState } from 'react';
import { useResource } from '../requests';
import { Button } from '../ui';
import { subscribeChanges } from '../chat/live';
import { RequestRow } from './request-row';
import { loadRequests, type AgentRequest } from './request-model';
const noPages: number[] = [];

export function useAgentRequests(agent: string, scope: string, status: string, cursors = noPages) {
  const load = useCallback(async () => {
    const pages = await Promise.all([loadRequests(agent, scope, status), ...cursors.map(cursor => loadRequests(agent, scope, status, cursor))]);
    const records = new Map<string, AgentRequest>();
    for (const page of pages) for (const request of page.requests) records.set(request.id, request);
    return { requests: [...records.values()].sort((a, b) => b.sequence - a.sequence), next_before: pages.at(-1)!.next_before };
  }, [agent, scope, status, cursors]);
  const resource = useResource(load);
  useEffect(() => {
    const timer = window.setInterval(() => { if (!document.hidden) void resource.refresh().catch(() => {}); }, 5000);
    const unsubscribe = subscribeChanges(change => {
      if (['access', 'reset','event_access'].includes(change.kind)) { resource.setData(undefined); void resource.refresh().catch(() => {}); }
    });
    return () => { clearInterval(timer); unsubscribe(); };
  }, [resource.refresh, resource.setData]);
  return resource;
}

export function AgentWork({ agent, activity = false }: { agent: string; activity?: boolean }) {
  const [scope, setScope] = useState('mine');
  const [status, setStatus] = useState(activity ? 'all' : 'active');
  return <section className="agent-work" aria-label={activity ? 'Request history' : 'Active requests'}>
    <header className="agent-section-heading"><div><h3>{activity ? 'Request history' : 'Active requests'}</h3>
      <p className="ui-help">Progress comes from saved work. Stop cancels one request; pausing an agent is a separate control.</p></div></header>
    <div className="agent-work-filters"><label>Show<select aria-label="Show requests" value={scope} onChange={e => setScope(e.target.value)}>
      <option value="mine">My requests</option><option value="visible">All work I can view</option>
    </select></label><label>Status<select aria-label="Request status" value={status} onChange={e => setStatus(e.target.value)}>
      <option value="active">Active</option><option value="all">Active and finished</option>
    </select></label></div>
    <RequestList key={`${agent}:${scope}:${status}`} agent={agent} scope={scope} status={status} />
  </section>;
}

function RequestList({ agent, scope, status }: { agent: string; scope: string; status: string }) {
  const [cursors, setCursors] = useState<number[]>([]);
  const { data, error, refresh } = useAgentRequests(agent, scope, status, cursors);
  const cursor = data?.next_before;
  return <><div className="agent-refresh"><Button onClick={() => void refresh().catch(() => {})}>Refresh progress</Button>
      <small>Shows work you can currently access.</small></div>
    {error && <p role="alert">{error}</p>}
    {!data && !error && <p role="status">Loading requests…</p>}
    {data && !data.requests.length && <p className="ui-empty">{status === 'active' ? 'No active requests in this view.' : 'No requests in this view yet.'}</p>}
    {data?.requests.map(request => <RequestRow key={request.id} request={request} refresh={refresh} />)}
    {cursor && <Button disabled={cursors.includes(cursor)} onClick={() => setCursors(old => old.includes(cursor) ? old : [...old, cursor])}>Load more requests</Button>}
  </>;
}

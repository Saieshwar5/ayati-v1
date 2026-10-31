import { useCallback, useEffect } from 'react';
import { useResource } from '../requests';
import { subscribeChanges } from '../chat/live';
import { loadRequests, type AgentRequest } from './request-model';
const noPages: number[] = [];

/** One agent's requests for the viewer, refreshed every 5 seconds and cleared
 * when access changes. The Overview shows the open ones. */
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

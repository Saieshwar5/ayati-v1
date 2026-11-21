import { workspaceUrl } from '../../workspace-context';
import { CoordinationView } from '../chat/handoffs';
import { requestLabels, requestUrl, type AgentRequest } from './request-model';

const waiting = ['pending', 'executing', 'uncertain'];

/**
 * One request on an agent's page. The page shows work and leads to it; it never
 * decides it (2026-11-21): an approval is answered in Activity → Needs me, or in
 * the conversation, and a request is stopped there too.
 */
export function RequestRow({ request, refresh, compact = false }: {
  request: AgentRequest; refresh: () => Promise<void>; compact?: boolean;
}) {
  return <article className="agent-work-row" aria-label={`Request: ${request.summary}`}>
    <div className="agent-work-heading"><strong>{request.summary}</strong>
      <span className={`agent-work-state ${request.state}`}>{requestLabels[request.state] ?? request.state}</span>
    </div>
    <p className="agent-work-meta">{request.conversation_name}{request.thread && ' · Thread'} · {request.requester_name ?? 'Former member'}
      <time dateTime={new Date(request.created_ms).toISOString()}> · {new Date(request.created_ms).toLocaleString()}</time>
    </p>
    {!compact && request.latest_update && <p className="agent-work-update">{request.latest_update}</p>}
    <CoordinationView data={request.coordination} refresh={refresh}/>
    {request.cancellation && <p className="ui-help">Cancellation recorded by {request.cancellation.by}. Effects already dispatched retain their outcomes.</p>}
    <div className="agent-work-actions"><a className="ui-button" href={requestUrl(request)}>Open conversation</a>
      {request.approvals.some(approval => waiting.includes(approval.state)) &&
        <a className="ui-button" href={workspaceUrl('/?view=activity&tab=needs')}>Review in Activity</a>}
    </div>
  </article>;
}

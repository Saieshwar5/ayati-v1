import { useState } from 'react';
import { Button } from '../ui';
import { useMutation } from '../requests';
import { ApprovalDialog } from '../approvals/approval-dialog';
import { CoordinationView } from '../chat/handoffs';
import { cancelRequest, requestLabels, requestUrl, type AgentRequest } from './request-model';

export function RequestRow({ request, refresh, compact = false }: {
  request: AgentRequest; refresh: () => Promise<void>; compact?: boolean;
}) {
  const mutation = useMutation();
  const [approval, review] = useState<string>();
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
      {request.approvals.filter(a => ['pending', 'executing', 'uncertain'].includes(a.state)).map(a =>
        <Button key={a.id} onClick={() => review(a.id)}>Review {a.title}</Button>)}
      {request.can_cancel && <Button disabled={mutation.busy} onClick={() => void mutation.run(async () => {
        await cancelRequest(request); await refresh();
      })}>{mutation.busy ? 'Saving cancellation…' : 'Stop request'}</Button>}
    </div>
    {mutation.status && <p role="alert">{mutation.status}</p>}
    {approval && <ApprovalDialog id={approval} close={() => { review(undefined); void refresh().catch(() => {}); }} />}
  </article>;
}

import { useCallback, useEffect, useId, useRef } from 'react';
import { Button } from '../ui';
import { useMutation, useResource } from '../requests';
import { workspaceUrl } from '../../workspace-context';
import { AgentFace } from '../faces';
import { chatRequest, type Conversation } from './model';
import { alertText } from './notifications';
import type { Lifecycle, Unfinished } from './organization-model';
import { subscribeChanges } from './live';

/** Archive or restore review, shown in place inside Details (D245 screen 6). */
export function ArchiveReview({ chat, close, changed }: { chat: Conversation; close: () => void; changed: () => Promise<void> }) {
  const load = useCallback(() => chatRequest<Lifecycle>(`/chats/${chat.id}/lifecycle`), [chat.id]);
  const resource = useResource(load), mutation = useMutation();
  const id = useId();
  const heading = useRef<HTMLHeadingElement>(null);
  useEffect(() => { heading.current?.focus(); }, []);
  useEffect(() => subscribeChanges(change => { if (['access', 'reset'].includes(change.kind)) void resource.refresh().catch(() => {}); }), [resource.refresh]);
  // One request identity per exact change, so a retry after a lost reply cannot repeat it.
  const attempt = useRef<{ signature: string; request: string }>(undefined);
  const confirm = () => {
    if (!resource.data?.manageable) return;
    const payload = { archived: !resource.data.archived, revision: resource.data.revision };
    const signature = JSON.stringify(payload);
    if (attempt.current?.signature !== signature) attempt.current = { signature, request: crypto.randomUUID() };
    const request = attempt.current.request;
    void mutation.run(async () => { await chatRequest(`/chats/${chat.id}/lifecycle`, { ...payload, request }); await changed(); close(); });
  };
  const state = resource.data;
  const archived = state?.archived ?? Boolean(chat.archived);
  const running = state?.active_requests ?? 0;
  return <section className="chat-archive-review" aria-labelledby={id} aria-busy={mutation.busy}>
    <h3 id={id} ref={heading} tabIndex={-1}>{archived ? `Restore #${chat.name}?` : `Archive #${chat.name}?`}</h3>
    <p>{archived
      ? 'People can post and ask agents again. Stopped work stays stopped and nothing restarts on its own.'
      : 'History and files stay. Nobody can post and agents take no new requests. Restore opens it again.'}</p>
    {!state && !resource.error && <p className="chat-help" role="status">Checking unfinished work…</p>}
    {resource.error && <p className="chat-error" role="alert">{resource.error}</p>}
    {state && <div className="chat-archive-running">
      <strong>{running ? `Still running · ${running}` : 'Nothing is still running'}</strong>
      <Button className="chat-archive-check" disabled={mutation.busy} onClick={() => { mutation.setStatus(''); void resource.refresh().catch(() => {}); }}>Check again</Button>
      {Boolean(state.unfinished?.length) && <ul className="chat-archive-items" aria-label={`Unfinished requests in #${chat.name}`}>
        {state.unfinished!.map(item => <UnfinishedRow key={item.request} item={item} />)}
        {Boolean(state.more) && <li className="chat-archive-more">+{state.more} more</li>}
      </ul>}
      {running > 0 && <p>{running === 1 ? 'One visible request has' : `${running} visible requests have`} not finished. {archived
        ? 'Restore does not change them.'
        : 'Accepted work can finish; its questions, approvals and Stop stay available. Stop it from its message if it should not.'}</p>}
    </div>}
    {state && !state.manageable && <p className="chat-help">Only the channel creator or a workspace admin with channel access can change this.</p>}
    {mutation.status && <p className="chat-error" role="alert">{mutation.status}</p>}
    <div className="chat-archive-review-actions">
      <Button primary className={archived ? '' : 'chat-archive-confirm'} disabled={mutation.busy || !state?.manageable} onClick={confirm}>
        {mutation.busy ? (archived ? 'Restoring…' : 'Archiving…') : archived ? 'Restore channel' : 'Archive channel'}
      </Button>
      <Button disabled={mutation.busy} onClick={close}>Cancel</Button>
    </div>
  </section>;
}

const stateLabels: Record<Unfinished['state'], string> = {
  queued: 'starting', running: 'working', waiting: 'waiting', approval: 'waiting for approval', question: 'waiting for an answer',
};

/** One unfinished request: what was asked, the agent, its state and Open in Activity. */
function UnfinishedRow({ item }: { item: Unfinished }) {
  const agent = item.agent_name ?? 'Agent';
  const asked = item.summary ? alertText(item.summary) || 'A request' : 'A request';
  const waiting = item.state === 'waiting' || item.state === 'approval' || item.state === 'question';
  return <li className="chat-archive-item">
    <AgentFace agent={{ id: item.agent, name: agent, avatar: item.agent_avatar }} />
    <span>
      <span className="chat-archive-asked">{asked}</span>
      <small>{agent} · <span className={waiting ? 'chat-archive-waiting' : 'chat-archive-working'}>{stateLabels[item.state] ?? item.state}</span></small>
    </span>
    <a href={workspaceUrl(`/?view=activity&tab=history&run=${encodeURIComponent(item.request)}`)} aria-label={`Open “${asked}” in Activity`}>Open</a>
  </li>;
}

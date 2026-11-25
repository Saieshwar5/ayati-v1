import { useEffect, useRef, useState } from 'react';
import { Button } from '../ui';
import { errorMessage, useMutation } from '../requests';
import { workspaceSession } from '../../workspace-context';
import { openAgentChat } from '../chat/direct-conversation';
import { attentionKey, type AttentionItem } from '../attention/model';
import { AddNote } from './add-note';
import { clock, conversationUrl, dayLabel, loadDetails, steerRequest, where, type Details } from './model';

/** The conversation an approval or question came from, shown under it in the reading
 * pane (designs/activity-next, Design A): the asking message and the agent's reply
 * from Cuckoo's own records, a way to add to your own running request, and the two
 * ways into Chat. Someone who is not in the conversation is not shown it by itself:
 * reading it is recorded, so that stays their choice. */
export function RequestTalk({ item, member }: { item: AttentionItem; member: boolean }) {
  const [wanted, setWanted] = useState(member);
  const [state, setState] = useState<{ data?: Details; error?: string }>({});
  const [version, setVersion] = useState(0);
  const mutation = useMutation();
  const direct = useRef<string>(undefined);
  useEffect(() => {
    if (!wanted) return;
    const controller = new AbortController();
    loadDetails(item.reply, controller.signal)
      .then(data => { if (!controller.signal.aborted) setState({ data }); })
      .catch(error => { if (!controller.signal.aborted) setState({ error: errorMessage(error) }); });
    return () => controller.abort();
  }, [item.reply, wanted, version]);
  const data = state.data;
  const privately = <Button disabled={mutation.busy} onClick={() => void mutation.run(() =>
    openAgentChat({ id: item.agent, name: item.agent_name }, `About "${item.title}" (${item.conversation_name}): `, direct))}>Ask {item.agent_name} privately</Button>;
  if (!wanted) return <section className="activity-talk" aria-label="Conversation"><h3>Conversation</h3>
    <p className="ui-help">You are not in {item.conversation_name}. As one of this agent’s owners or an admin you can read this request; reading it is recorded in the audit log.</p>
    <div className="activity-detail-actions"><Button onClick={() => setWanted(true)}>Read the request</Button>{privately}</div>
    {mutation.status && <p role="alert">{mutation.status}</p>}</section>;
  const mine = data && data.requester === workspaceSession?.user?.id && ['queued', 'running', 'waiting', 'streaming'].includes(data.state);
  const back = `&from=activity&item=${encodeURIComponent(attentionKey(item))}`;
  return <section className="activity-talk" aria-label="Conversation">
    <header><h3>Conversation{data ? ` · ${where(data)}` : ''}</h3>
      {data?.in_conversation && <a href={conversationUrl(data) + back}>Open in Chat ›</a>}</header>
    {state.error && <p role="alert">{state.error}</p>}
    {!data && !state.error && <p role="status">Loading the conversation…</p>}
    {data && <div className="activity-talk-messages">
      {data.message && <div><strong>{data.requester_name ?? 'Someone'}</strong><small>{dayLabel(data.created_ms)} {clock(data.created_ms)}</small><blockquote>{data.message}</blockquote></div>}
      {data.reply && <div><strong>{data.agent_name}</strong><small>AI</small><blockquote>{data.reply}</blockquote></div>}
      {!data.message && !data.reply && <p className="ui-help">No messages are saved for this request yet.</p>}
    </div>}
    {data && !data.in_conversation && <p className="ui-help">You are not in this conversation, so there is nothing to open in Chat. Your view is recorded in the audit log.</p>}
    {mine && <AddNote label="Add to this request" send={text => steerRequest(data.id, text)} sent={() => setVersion(v => v + 1)} />}
    <div className="activity-detail-actions">{privately}</div>
    {mutation.status && <p role="alert">{mutation.status}</p>}
  </section>;
}

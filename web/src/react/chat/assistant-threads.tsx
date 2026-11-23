import { navigate } from '../../navigation';
import { workspaceUrl } from '../../workspace-context';
import { Button } from '../ui';
import { useMutation } from '../requests';
import { positionOf, setThreadState, threadIn, useThreads, type AssistantThread } from './assistant-threads-model';
import './assistant-threads.css';

const chatUrl = (chat: string) => workspaceUrl(`/?view=chat&conversation=${encodeURIComponent(chat)}`);
const withName = (thread: AssistantThread) => thread.name ?? (thread.kind === 'agent' ? 'an agent' : 'a member');

/** The one mark for what your assistant does for you: writing a reply, working in a chat, and where it is turned on. */
export function AssistantMark({ size = 14 }: { size?: number }) {
  return <svg className="assistant-mark" viewBox="0 0 24 24" width={size} height={size} fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M11 4l1.8 5.2L18 11l-5.2 1.8L11 18l-1.8-5.2L4 11l5.2-1.8z" /><path d="M19 16v4M17 18h4" />
  </svg>;
}

/** Start, turn off or remove one thread; one request at a time. */
function useThreadActions() {
  const { reload } = useThreads();
  const mutation = useMutation();
  const set = (thread: AssistantThread, state: 'running' | 'stopped' | 'deleted') => void mutation.run(async () => {
    await setThreadState(thread.id, state);
    reload();
  });
  return { ...mutation, set };
}

/** In the chat list: the mark on a chat your assistant works in. Green while it
 * replies for you, grey while it writes replies for you to send. */
export function ThreadMark({ chat, assistantName }: { chat: string; assistantName: string }) {
  const position = positionOf(threadIn(useThreads().threads, chat));
  if (position === 'off') return null;
  const label = position === 'auto' ? `${assistantName} is replying for you` : `${assistantName} asks you first`;
  return <span className={`chat-thread-mark ${position}`} role="img" aria-label={label} title={label}><AssistantMark /></span>;
}

/** In the conversation, above the first message your assistant answers for you. Only you see it. */
export function TookOverLine({ assistantName }: { assistantName: string }) {
  return <p className="chat-took-over" title="Only you see this"><span><AssistantMark size={12} />{assistantName} replies for you from here</span></p>;
}

/** At the top of your assistant's own chat: every chat it replies in for you, and what it suggests. */
export function AssistantRoom({ assistantName }: { assistantName: string }) {
  const { threads } = useThreads();
  const actions = useThreadActions();
  // Chats it works in now, and ones it had to stop in, with the reason.
  const working = threads.filter(thread => thread.state === 'running' && thread.chat);
  const halted = threads.filter(thread => thread.state === 'stopped' && thread.chat && thread.detail);
  const suggestions = threads.filter(thread => thread.state === 'suggested');
  if (!working.length && !halted.length && !suggestions.length) return null;
  const doing = (thread: AssistantThread) => thread.working ? 'Writing a reply…'
    : positionOf(thread) === 'auto' ? 'Replying for you'
    : thread.draft?.state === 'complete' ? 'Asks you first · a reply is ready' : 'Asks you first';
  return <div className="chat-thread-banners">
    {working.length + halted.length > 0 && <section className="chat-thread-strip" aria-label={`Chats ${assistantName} works in for you`}>
      <h3><AssistantMark />{working.length ? `Working in ${working.length} of your ${working.length === 1 ? 'chat' : 'chats'}` : 'Not working in your chats now'}</h3>
      {working.map(thread => <div className="chat-thread-item" key={thread.id}>
        <span><strong>{withName(thread)}</strong><small>{doing(thread)}{thread.instruction ? ` · ${thread.instruction}` : ''}</small></span>
        <Button onClick={() => navigate(chatUrl(thread.chat!))}>Open</Button>
        <Button disabled={actions.busy} onClick={() => actions.set(thread, 'stopped')}>Turn off</Button>
      </div>)}
      {halted.map(thread => <div className="chat-thread-item" key={thread.id}>
        <span><strong>{withName(thread)}</strong><small>{thread.detail}</small></span>
        <Button onClick={() => navigate(chatUrl(thread.chat!))}>Open</Button>
        <Button disabled={actions.busy} onClick={() => actions.set(thread, 'deleted')}>Dismiss</Button>
      </div>)}
    </section>}
    {suggestions.map(thread => <div key={thread.id} className="chat-thread-banner suggested">
      <AssistantMark />
      <span><strong>{assistantName} suggests talking to {withName(thread)} for you</strong> · {thread.title}
        <q>{thread.opening}</q>
        <small>If you start it, this is sent as your message, and {assistantName} keeps replying for you until the matter is settled or you turn it off.</small></span>
      <Button primary disabled={actions.busy} onClick={() => actions.set(thread, 'running')}>Start</Button>
      <Button disabled={actions.busy} onClick={() => actions.set(thread, 'deleted')}>Dismiss</Button>
    </div>)}
    {actions.status && <p className="chat-error" role="alert">{actions.status}</p>}
  </div>;
}

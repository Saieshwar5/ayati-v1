import { navigate } from '../../navigation';
import { workspaceUrl } from '../../workspace-context';
import { Button } from '../ui';
import { useMutation } from '../requests';
import { setThreadState, threadIn, useThreads, type AssistantThread } from './assistant-threads-model';
import './assistant-threads.css';

const chatUrl = (chat: string) => workspaceUrl(`/?view=chat&conversation=${encodeURIComponent(chat)}`);
const withName = (thread: AssistantThread) => thread.name ?? (thread.kind === 'agent' ? 'an agent' : 'a member');

/** The one mark for what your assistant does for you: drafting, taking a chat over, and what it wrote. */
export function AssistantMark({ size = 14 }: { size?: number }) {
  return <svg className="assistant-mark" viewBox="0 0 24 24" width={size} height={size} fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    <path d="M11 4l1.8 5.2L18 11l-5.2 1.8L11 18l-1.8-5.2L4 11l5.2-1.8z" /><path d="M19 16v4M17 18h4" />
  </svg>;
}

/** Stop, resume, start or delete one thread; one request at a time. */
function useThreadActions() {
  const { reload } = useThreads();
  const mutation = useMutation();
  const set = (thread: AssistantThread, state: 'running' | 'stopped' | 'deleted') => void mutation.run(async () => {
    await setThreadState(thread.id, state);
    reload();
  });
  return { ...mutation, set };
}

function ThreadButtons({ thread, actions }: { thread: AssistantThread; actions: ReturnType<typeof useThreadActions> }) {
  return thread.state === 'running'
    ? <Button disabled={actions.busy} onClick={() => actions.set(thread, 'stopped')}>Stop</Button>
    : <><Button disabled={actions.busy} onClick={() => actions.set(thread, 'running')}>Resume</Button><Button disabled={actions.busy} onClick={() => actions.set(thread, 'deleted')}>Delete</Button></>;
}

/** In the chat list: a mark on a chat your assistant is replying in, grey once stopped. */
export function ThreadMark({ chat, assistantName }: { chat: string; assistantName: string }) {
  const thread = threadIn(useThreads().threads, chat);
  if (!thread) return null;
  const label = thread.state === 'running' ? `${assistantName} is replying for you` : `${assistantName} stopped replying here`;
  return <span className={`chat-thread-mark ${thread.state}`} role="img" aria-label={label} title={label}><AssistantMark /></span>;
}

/** In a chat's header: your assistant is replying there, with Stop; or stopped, with Resume and Delete. */
export function ThreadPill({ chat, assistantName }: { chat: string; assistantName: string }) {
  const thread = threadIn(useThreads().threads, chat);
  const actions = useThreadActions();
  if (!thread) return null;
  const running = thread.state === 'running';
  return <div className={`chat-thread-pill ${thread.state}`} role="status">
    <details>
      <summary title="What you asked"><AssistantMark /><span>{assistantName} {running ? (thread.working ? 'is writing…' : 'is replying') : 'stopped'}</span></summary>
      <div className="chat-thread-about">
        <small>What you asked</small><p>{thread.instruction || thread.title}</p>
        <small>{thread.sent} {thread.sent === 1 ? 'message' : 'messages'} sent as you. You can write here too.</small>
        {!running && <p>{thread.detail || `${assistantName} sends nothing until you resume it.`}</p>}
      </div>
    </details>
    <ThreadButtons thread={thread} actions={actions} />
    {actions.status && <p className="chat-error" role="alert">{actions.status}</p>}
  </div>;
}

/** In the conversation, above the message a chat was handed over from. Only you see it. */
export function TookOverLine({ assistantName }: { assistantName: string }) {
  return <p className="chat-took-over" title="Only you see this"><span><AssistantMark size={12} />{assistantName} took over from here</span></p>;
}

/** At the top of your assistant's own chat: every chat it replies in for you, and what it suggests. */
export function AssistantRoom({ assistantName }: { assistantName: string }) {
  const { threads } = useThreads();
  const actions = useThreadActions();
  const handed = threads.filter(thread => thread.state !== 'suggested' && thread.chat);
  const suggestions = threads.filter(thread => thread.state === 'suggested');
  if (!handed.length && !suggestions.length) return null;
  const running = handed.filter(thread => thread.state === 'running').length;
  return <div className="chat-thread-banners">
    {handed.length > 0 && <section className="chat-thread-strip" aria-label={`Chats ${assistantName} replies in for you`}>
      <h3><AssistantMark />{running ? `Replying for you in ${running} ${running === 1 ? 'chat' : 'chats'}` : 'Not replying for you anywhere now'}</h3>
      {handed.map(thread => <div className="chat-thread-item" key={thread.id}>
        <span><strong>{withName(thread)}</strong>
          <small>{thread.state === 'running' ? (thread.working ? 'Writing a reply…' : 'Replying') : thread.detail || 'Stopped'} · {thread.title}</small></span>
        <Button onClick={() => navigate(chatUrl(thread.chat!))}>Open</Button>
        <ThreadButtons thread={thread} actions={actions} />
      </div>)}
    </section>}
    {suggestions.map(thread => <div key={thread.id} className="chat-thread-banner suggested">
      <AssistantMark />
      <span><strong>{assistantName} suggests talking to {withName(thread)} for you</strong> · {thread.title}
        <q>{thread.opening}</q>
        <small>If you start it, this is sent as your message, and {assistantName} keeps replying for you until you stop it.</small></span>
      <Button primary disabled={actions.busy} onClick={() => actions.set(thread, 'running')}>Start</Button>
      <Button disabled={actions.busy} onClick={() => actions.set(thread, 'deleted')}>Dismiss</Button>
    </div>)}
    {actions.status && <p className="chat-error" role="alert">{actions.status}</p>}
  </div>;
}
